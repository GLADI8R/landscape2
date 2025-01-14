import orderBy from 'lodash/orderBy';
import { memo, useContext } from 'react';
import { useNavigate } from 'react-router-dom';
import { Waypoint } from 'react-waypoint';

import { COLORS } from '../../../data';
import { BaseItem, CardMenu, Item } from '../../../types';
import arePropsEqual from '../../../utils/areEqualProps';
import convertStringSpaces from '../../../utils/convertStringSpaces';
import isElementInView from '../../../utils/isElementInView';
import { CategoriesData } from '../../../utils/prepareData';
import { ActionsContext, AppActionsContext } from '../../context/AppContext';
import Card from './Card';
import styles from './Content.module.css';

interface Props {
  menu: CardMenu;
  data: CategoriesData;
  isVisible: boolean;
}

interface WaypointProps {
  id: string;
  children: JSX.Element;
  isVisible: boolean;
}

const WaypointItem = (props: WaypointProps) => {
  const navigate = useNavigate();

  const handleEnter = () => {
    if (`#${props.id}` !== location.hash && props.isVisible) {
      navigate(
        { ...location, hash: props.id },
        {
          replace: true,
        }
      );

      if (!isElementInView(`btn_${props.id}`)) {
        const target = window.document.getElementById(`btn_${props.id}`);
        if (target) {
          target.scrollIntoView({ block: 'nearest' });
        }
      }
    }
  };

  return (
    <Waypoint topOffset="20px" bottomOffset="97%" onEnter={handleEnter} fireOnRapidScroll={false}>
      {props.children}
    </Waypoint>
  );
};

const Content = memo(function Content(props: Props) {
  const bgColor = COLORS[0];
  const { updateActiveItemId } = useContext(AppActionsContext) as ActionsContext;

  const sortItems = (firstCategory: string, firstSubcategory: string): BaseItem[] => {
    return orderBy(
      props.data[firstCategory][firstSubcategory].items,
      [(item: BaseItem) => item.name.toLowerCase().toString()],
      'asc'
    );
  };

  return (
    <>
      {Object.keys(props.menu).map((cat: string) => {
        return (
          <div key={`list_cat_${cat}`}>
            {props.menu[cat].map((subcat: string) => {
              const id = convertStringSpaces(`${cat}/${subcat}`);

              return (
                <WaypointItem key={`list_subcat_${subcat}`} id={id} isVisible={props.isVisible}>
                  <div>
                    <div id={id} className={`d-flex flex-row fw-semibold mb-4 ${styles.title}`}>
                      <div
                        className={`text-white p-2 text-nowrap text-truncate ${styles.categoryTitle}`}
                        style={{ backgroundColor: bgColor }}
                      >
                        {cat}
                      </div>
                      <div className={`p-2 flex-grow-1 text-truncate ${styles.subcategoryTitle}`}>{subcat}</div>
                    </div>
                    <div className="row g-4 mb-4">
                      {sortItems(cat, subcat).map((item: Item) => {
                        return (
                          <div key={`card_${item.id}`} className="col-12 col-lg-6 col-xxl-4 col-xxxl-3">
                            <div
                              className={`card rounded-0 p-3 ${styles.card}`}
                              onClick={() => updateActiveItemId(item.id)}
                            >
                              <Card item={item} className="h-100" />
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                </WaypointItem>
              );
            })}
          </div>
        );
      })}
    </>
  );
}, arePropsEqual);

export default Content;
