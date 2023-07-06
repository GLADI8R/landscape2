import { BrowserRouter as Router, Route, Routes } from 'react-router-dom';

import './styles/default.scss';
import './App.css';
import Landscape from './layout/explore';
import Layout from './layout';
import NotFound from './layout/notFound';
import Stats from './layout/stats';
import Guide from './layout/guide';
import itemsDataGetter from './utils/itemsDataGetter';
import { useEffect } from 'react';

const App = () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const data = (window as any).baseDS;

  useEffect(() => {
    itemsDataGetter.init();
  }, []);

  if (data === null || data === undefined) return null;

  return (
    <Router>
      <Routes>
        <Route path="/" element={<Layout items={data.items} />}>
          <Route index element={<Landscape data={data} />} />
          <Route path="/guide" element={<Guide />} />
          <Route path="/stats" element={<Stats />} />
          <Route path="*" element={<NotFound />} />
        </Route>
      </Routes>
    </Router>
  );
};

export default App;