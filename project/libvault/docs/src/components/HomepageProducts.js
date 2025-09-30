import React from 'react';
import clsx from 'clsx';
import Slider from 'react-slick';
import styles from './HomepageProducts.module.css';

import 'slick-carousel/slick/slick.css';
import 'slick-carousel/slick/slick-theme.css';

const ProductList = [
  {
    url: 'https://caido.io/',
    logo: 'img/other/caido-logo.png',
    desc: 'A lightweight web security auditing toolkit',
    logoClassName: null,
  },
  {
    url: 'https://mydatamyconsent.com/',
    logo: 'img/other/mydatamyconsent-logo.png',
    desc: 'Online data sharing simplified',
    logoClassName: styles.mydatamyconsentLogo,
  },
  {
    url: 'https://prefix.dev/',
    logo: 'img/other/prefixdev-logo.png',
    desc: 'Rethinking Package Management',
    logoClassName: styles.prefixdevLogo,
  },
  {
    url: 'https://www.svix.com/',
    logo: 'img/other/svix-logo.svg',
    desc: 'The enterprise ready webhooks service',
    logoClassName: styles.svixLogo,
  },
  {
    url: 'https://upvpn.app/',
    logo: 'img/other/upvpn-logo.png',
    desc: 'Serverless Pay as you go VPN',
    logoClassName: styles.upvpnLogo,
  },
];

function Product({url, logo, desc, logoClassName}) {
  return (
    <div style={{ height: '180px' }}>
      <a href={url} target="_blank" className={clsx(styles.anchorNormalText)}>
        <div style={{
          paddingBottom: '20px',
          paddingTop: '20px',
          height: '100%',
          textAlign: 'center',
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'flex-end',
        }}>
          <div style={{ display: 'flex', justifyContent: 'center' }}>
            <img src={`${logo}?t=${Date.now()}`} className={clsx(logoClassName)} style={{ width: '250px' }}/>
          </div>
          <p style={{ margin: '0px', marginTop: '20px' }}>
            {desc}
          </p>
        </div>
      </a>
    </div>
  );
}

const settings = {
  dots: false,
  infinite: true,
  pauseOnHover: true,
  swipeToSlide: true,
  slidesToShow: 4,
  slidesToScroll: 1,
  initialSlide: Math.floor(Math.random() * ProductList.length),
  rows: 1,
  autoplay: true,
  speed: 500,
  autoplaySpeed: 3000,
  responsive: [
    { breakpoint: 1680, settings: { slidesToShow: 3 } },
    { breakpoint: 900, settings: { slidesToShow: 2 } },
    { breakpoint: 650, settings: { slidesToShow: 1, rows: 2 } },
  ]
};

export default function HomepageProducts() {
  return (
    <section id="our-users" className={clsx('home-section', styles.features)}>
      <div className="container">
        <div className="row">
          <div className="col col--12">
              <h2 className="text--center">Who's using RustyVault?</h2>
              <br/>
              <p className="text--center">
                The following startups are using RustyVault:
              </p>
          </div>
        </div>
        <div className="row">
          <div className="col col--12">
            <Slider {...settings}>
              {ProductList.map((props, idx) => (
                <Product key={idx} {...props} />
              ))}
            </Slider>
          </div>
        </div>
        <br/>
        <div className="row">
          <div className="col col--12">
            <p className="text--center">
              For more projects, see <a href="https://github.com/SeaQL/sea-orm/blob/master/COMMUNITY.md#built-with-seaorm" target="_blank">Built with RustyVault</a>.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
