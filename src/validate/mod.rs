//! This module defines the functionality of the validate CLI subcommand.

use crate::{build::LandscapeData, DataSource};
use anyhow::{Context, Result};
use tracing::instrument;

/// Validate landscape data file.
#[instrument(skip_all)]
pub(crate) async fn validate_data(data_source: &DataSource) -> Result<()> {
    LandscapeData::new(data_source)
        .await
        .context("the landscape data file provided is not valid")?;

    println!("The landscape data file provided is valid!");
    Ok(())
}
