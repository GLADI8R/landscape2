import { useState } from 'react';

import { SVGIconKind } from '../../types';
import SVGIcon from './SVGIcon';

interface Props {
  name: string;
  logo: string;
  className?: string;
}

const Image = (props: Props) => {
  const [error, setError] = useState(false);

  return (
    <>
      {error ? (
        <SVGIcon kind={SVGIconKind.NotImage} className={`opacity-25 ${props.className}`} />
      ) : (
        <img
          alt={`${props.name} logo`}
          className={props.className}
          src={import.meta.env.MODE === 'development' ? `../../static/${props.logo}` : `${props.logo}`}
          onError={() => setError(true)}
        />
      )}
    </>
  );
};

export default Image;
