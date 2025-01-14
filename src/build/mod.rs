//! This module defines the functionality of the build CLI subcommand.

#![allow(non_upper_case_globals)]

use self::{
    cache::Cache,
    crunchbase::collect_crunchbase_data,
    datasets::Datasets,
    export::generate_items_csv,
    github::collect_github_data,
    guide::LandscapeGuide,
    logos::prepare_logo,
    projects::{generate_projects_csv, Project, ProjectsMd},
    settings::{Images, LandscapeSettings},
};
use crate::{BuildArgs, GuideSource, LogosSource};
use anyhow::{format_err, Context, Result};
use askama::Template;
use futures::stream::{self, StreamExt};
use reqwest::StatusCode;
use rust_embed::RustEmbed;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::Path,
    sync::Arc,
    time::Instant,
};
use tracing::{debug, error, info, instrument};
use url::Url;
use uuid::Uuid;

mod cache;
mod crunchbase;
mod data;
mod datasets;
mod export;
mod github;
mod guide;
mod logos;
mod projects;
mod settings;
pub(crate) use data::LandscapeData;

/// Path where the datasets will be written to in the output directory.
const DATASETS_PATH: &str = "data";

/// Path where some documents will be written to in the output directory.
const DOCS_PATH: &str = "docs";

/// Path where some images will be written to in the output directory.
const IMAGES_PATH: &str = "images";

/// Path where the item logos will be written to in the output directory.
const LOGOS_PATH: &str = "logos";

/// Maximum number of logos to prepare concurrently.
const PREPARE_LOGOS_MAX_CONCURRENCY: usize = 20;

/// Embed web application assets into binary.
/// (these assets will be built automatically from the build script)
#[derive(RustEmbed)]
#[folder = "web/dist"]
struct WebAssets;

/// Build landscape website.
#[instrument(skip_all)]
pub(crate) async fn build(args: &BuildArgs) -> Result<()> {
    info!("building landscape website..");
    let start = Instant::now();

    // Check required web assets are present
    check_web_assets()?;

    // Setup output directory, creating it when needed
    setup_output_dir(&args.output_dir)?;

    // Setup cache
    let cache = Cache::new(&args.cache_dir)?;

    // Get landscape data from the source provided
    let mut landscape_data = LandscapeData::new(&args.data_source).await?;

    // Get landscape settings from the source provided
    let mut settings = LandscapeSettings::new(&args.settings_source).await?;

    // Add some extra information to the landscape based on the settings
    landscape_data.add_featured_items_data(&settings)?;
    landscape_data.add_member_subcategory(&settings.members_category);

    // Get settings images and update their urls to the local copy
    settings.images = get_settings_images(&settings, &args.output_dir).await?;

    // Prepare guide and copy it to the output directory
    let includes_guide = prepare_guide(&args.guide_source, &args.output_dir).await?.is_some();

    // Prepare items logos and copy them to the output directory
    prepare_items_logos(&cache, &args.logos_source, &mut landscape_data, &args.output_dir).await?;

    // Collect data from external services
    let (crunchbase_data, github_data) = tokio::try_join!(
        collect_crunchbase_data(&cache, &landscape_data),
        collect_github_data(&cache, &landscape_data)
    )?;

    // Add data collected from external services to the landscape data
    landscape_data.add_crunchbase_data(crunchbase_data)?;
    landscape_data.add_github_data(github_data)?;

    // Generate datasets for web application
    let datasets = generate_datasets(&landscape_data, &settings, includes_guide, &args.output_dir)?;

    // Render index file and write it to the output directory
    render_index(&datasets, &args.output_dir)?;

    // Copy web assets files to the output directory
    copy_web_assets(&args.output_dir)?;

    // Generate items.csv file
    generate_items_csv_file(&landscape_data, &args.output_dir)?;

    // Generate projects.* files
    generate_projects_files(&landscape_data, &args.output_dir)?;

    let duration = start.elapsed().as_secs_f64();
    info!("landscape website built! (took: {:.3}s)", duration);

    Ok(())
}

/// Check web assets are present, to make sure the web app has been built.
#[instrument(skip_all, err)]
fn check_web_assets() -> Result<()> {
    debug!("checking web assets are present");

    if !WebAssets::iter().any(|path| path.starts_with("assets/")) {
        return Err(format_err!(
            "web assets not found, please make sure they have been built"
        ));
    }

    Ok(())
}

/// Copy web assets files to the output directory.
#[instrument(skip_all, err)]
fn copy_web_assets(output_dir: &Path) -> Result<()> {
    debug!("copying web assets to output directory");

    for asset_path in WebAssets::iter() {
        // The index document is a template that we'll render, so we don't want
        // to copy it as is.
        if asset_path == "index.html" || asset_path == ".keep" {
            continue;
        }

        if let Some(embedded_file) = WebAssets::get(&asset_path) {
            if let Some(parent_path) = Path::new(asset_path.as_ref()).parent() {
                fs::create_dir_all(output_dir.join(parent_path))?;
            }
            let mut file = File::create(output_dir.join(asset_path.as_ref()))?;
            file.write_all(&embedded_file.data)?;
        }
    }

    Ok(())
}

/// Generate datasets from the landscape data and settings, as well as from the
/// data collected from external services (GitHub, Crunchbase, etc). Some of
/// the datasets will be embedded in the index document, and the rest will be
/// written to the DATASETS_PATH in the output directory.
#[instrument(skip_all, err)]
fn generate_datasets(
    landscape_data: &LandscapeData,
    settings: &LandscapeSettings,
    includes_guide: bool,
    output_dir: &Path,
) -> Result<Datasets> {
    debug!("generating datasets");

    let datasets = Datasets::new(landscape_data, settings, includes_guide)?;
    let datasets_path = output_dir.join(DATASETS_PATH);

    // Base
    let mut base_file = File::create(datasets_path.join("base.json"))?;
    base_file.write_all(&serde_json::to_vec(&datasets.base)?)?;

    // Full
    let mut full_file = File::create(datasets_path.join("full.json"))?;
    full_file.write_all(&serde_json::to_vec(&datasets.full)?)?;

    Ok(datasets)
}

/// Generate the projects.md and projects.csv files from the landscape data.
#[instrument(skip_all, err)]
fn generate_projects_files(landscape_data: &LandscapeData, output_dir: &Path) -> Result<()> {
    debug!("generating projects files");

    let projects: Vec<Project> = landscape_data.into();

    // projects.md
    let projects_md = ProjectsMd { projects: &projects }.render()?;
    let docs_path = output_dir.join(DOCS_PATH);
    let mut file = File::create(docs_path.join("projects.md"))?;
    file.write_all(projects_md.as_bytes())?;

    // projects.csv
    let w = csv::Writer::from_path(docs_path.join("projects.csv"))?;
    generate_projects_csv(w, &projects)?;

    Ok(())
}

/// Generate the items.csv file from the landscape data.
#[instrument(skip_all, err)]
fn generate_items_csv_file(landscape_data: &LandscapeData, output_dir: &Path) -> Result<()> {
    debug!("generating items csv file");

    let docs_path = output_dir.join(DOCS_PATH);
    let w = csv::Writer::from_path(docs_path.join("items.csv"))?;
    generate_items_csv(w, landscape_data)?;

    Ok(())
}

/// Get settings images and copy them to the output directory.
#[instrument(skip_all, err)]
async fn get_settings_images(settings: &LandscapeSettings, output_dir: &Path) -> Result<Images> {
    // Helper function to process the image provided
    async fn process_image(url: &Option<String>, output_dir: &Path) -> Result<Option<String>> {
        let Some(url) = url else {
            return Ok(None);
        };

        // Fetch image from url
        let resp = reqwest::get(url).await?;
        if resp.status() != StatusCode::OK {
            return Err(format_err!(
                "unexpected status ({}) code getting logo {url}",
                resp.status()
            ));
        }
        let img = resp.bytes().await?.to_vec();

        // Write image to output dir
        let url = Url::parse(url).context("invalid image url")?;
        let Some(file_name) = url.path_segments().and_then(Iterator::last) else {
            return Err(format_err!("invalid image url: {url}"));
        };
        let img_path = Path::new(IMAGES_PATH).join(file_name);
        let mut file = fs::File::create(output_dir.join(&img_path))?;
        file.write_all(&img)?;

        Ok(Some(img_path.to_string_lossy().into_owned()))
    }

    debug!("getting settings images");

    let (favicon, footer_logo, header_logo) = tokio::try_join!(
        process_image(&settings.images.favicon, output_dir),
        process_image(&settings.images.footer_logo, output_dir),
        process_image(&settings.images.header_logo, output_dir),
    )?;
    let images = Images {
        favicon,
        footer_logo,
        header_logo,
        open_graph: settings.images.open_graph.clone(),
    };

    Ok(images)
}

/// Prepare guide and copy it to the output directory.
#[instrument(skip_all, err)]
async fn prepare_guide(guide_source: &GuideSource, output_dir: &Path) -> Result<Option<()>> {
    debug!("preparing guide");

    let Some(guide) = LandscapeGuide::new(guide_source).await? else {
        return Ok(None);
    };
    let path = output_dir.join(DATASETS_PATH).join("guide.json");
    File::create(path)?.write_all(&serde_json::to_vec(&guide)?)?;

    Ok(Some(()))
}

/// Prepare items logos and copy them to the output directory, updating the
/// logo reference on each landscape item.
#[instrument(skip_all, err)]
async fn prepare_items_logos(
    cache: &Cache,
    logos_source: &LogosSource,
    landscape_data: &mut LandscapeData,
    output_dir: &Path,
) -> Result<()> {
    debug!("preparing logos");

    // Get logos from the source and copy them to the output directory
    let mut concurrency = num_cpus::get();
    if concurrency > PREPARE_LOGOS_MAX_CONCURRENCY {
        concurrency = PREPARE_LOGOS_MAX_CONCURRENCY;
    }
    let http_client = reqwest::Client::new();
    let logos_source = Arc::new(logos_source.clone());
    let logos: HashMap<Uuid, Option<String>> = stream::iter(landscape_data.items.iter())
        .map(|item| async {
            // Prepare logo
            let cache = cache.clone();
            let http_client = http_client.clone();
            let logos_source = logos_source.clone();
            let file_name = item.logo.clone();
            let logo = match tokio::spawn(async move {
                prepare_logo(&cache, http_client, &logos_source, &file_name).await
            })
            .await
            {
                Ok(Ok(logo)) => logo,
                Ok(Err(err)) => {
                    error!(?err, ?item.logo, "error preparing logo");
                    return (item.id, None);
                }
                Err(err) => {
                    error!(?err, ?item.logo, "error executing prepare_logo task");
                    return (item.id, None);
                }
            };

            // Copy logo to output dir using the digest(+.svg) as filename
            let file_name = format!("{}.svg", logo.digest);
            let Ok(mut file) = fs::File::create(output_dir.join(LOGOS_PATH).join(&file_name)) else {
                error!(?file_name, "error creating logo file in output dir");
                return (item.id, None);
            };
            if let Err(err) = file.write_all(&logo.svg_data) {
                error!(?err, ?file_name, "error writing logo to file in output dir");
            };

            (item.id, Some(format!("{LOGOS_PATH}/{file_name}")))
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    // Update logo field in landscape items to logo digest path
    for item in &mut landscape_data.items {
        item.logo = if let Some(Some(logo)) = logos.get(&item.id) {
            logo.clone()
        } else {
            String::new()
        }
    }

    Ok(())
}

/// Template for the index document.
#[derive(Debug, Clone, Template)]
#[template(path = "index.html", escape = "none")]
struct Index<'a> {
    datasets: &'a Datasets,
}

/// Render index file and write it to the output directory.
#[instrument(skip_all, err)]
fn render_index(datasets: &Datasets, output_dir: &Path) -> Result<()> {
    debug!("rendering index.html file");

    let index = Index { datasets }.render()?;
    let mut file = File::create(output_dir.join("index.html"))?;
    file.write_all(index.as_bytes())?;

    Ok(())
}

/// Setup output directory, creating it as well as any of the other required
/// paths inside it when needed.
#[instrument(fields(?output_dir), skip_all, err)]
fn setup_output_dir(output_dir: &Path) -> Result<()> {
    debug!("setting up output directory");

    if !output_dir.exists() {
        debug!("creating output directory");
        fs::create_dir_all(output_dir)?;
    }

    let datasets_path = output_dir.join(DATASETS_PATH);
    if !datasets_path.exists() {
        fs::create_dir(datasets_path)?;
    }

    let docs_path = output_dir.join(DOCS_PATH);
    if !docs_path.exists() {
        fs::create_dir(docs_path)?;
    }

    let images_path = output_dir.join(IMAGES_PATH);
    if !images_path.exists() {
        fs::create_dir(images_path)?;
    }

    let logos_path = output_dir.join(LOGOS_PATH);
    if !logos_path.exists() {
        fs::create_dir(logos_path)?;
    }

    Ok(())
}

mod filters {
    use askama_escape::JsonEscapeBuffer;
    use serde::Serialize;

    /// Serialize to JSON.
    ///
    /// Based on the `json` built-in filter except the output is not pretty
    /// printed.
    pub(crate) fn json_compact<S: Serialize>(s: S) -> askama::Result<String> {
        let mut writer = JsonEscapeBuffer::new();
        serde_json::to_writer(&mut writer, &s)?;
        Ok(writer.finish())
    }
}
