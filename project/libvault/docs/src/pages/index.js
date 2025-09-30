import React from 'react';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Translate from '@docusaurus/Translate';
import styles from './index.module.css';
import HomepageFeatures from '../components/HomepageFeatures';
import HomepageCompare from '../components/HomepageCompare';
// import HomepageProducts from '../components/HomepageProducts';
// import HomepageMascot from '../components/HomepageMascot';
import {useColorMode} from '@docusaurus/theme-common';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  const {colorMode, setColorMode} = useColorMode();

  return (
    <header className={styles.heroBanner}>
      <div className="container">
      <img 
          className={styles.homepageBanner} 
          width="90%" 
          src={colorMode == "light" ? require('@site/static/img/RustyVault-home.gif').default : require('@site/static/img/RustyVault-home-Dark.gif').default}
      />
        {/* <h2 className="hero__subtitle">{siteConfig.tagline}</h2> */}
        <br/><a href="https://github.com/Tongsuo-Project/RustyVault" target="_blank"><img src="https://img.shields.io/github/stars/Tongsuo-Project/RustyVault.svg?style=social&label=Star"/></a>
        <br/><Translate description="Start">Every ⭐ counts!</Translate>
        <br/>
        <br/>
        <div className={styles.buttons}>
          <Link
            className="button button--primary button--lg"
            to="/docs/quick-start/">
            <Translate description="The Getting Started button">
            🔑 Getting Started
            </Translate>
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      description={siteConfig.tagline}>
      <HomepageHeader />
      <main>
        <HomepageFeatures />
        <HomepageCompare />
        {/* <HomepageProducts /> */}
        {/* <HomepageMascot /> */}
      </main>
    </Layout>
  );
}
