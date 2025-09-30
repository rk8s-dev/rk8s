import React from 'react';
import clsx from 'clsx';
import styles from './HomepageMascot.module.css';

export default function render() {
  return (
    <section className={clsx('home-section', styles.features)}>
      <div className="container">
        <div className="row">
          <div className={clsx('col col--12')}>
            <h2 className="text--center">Meet Terres, our official mascot</h2>
            <p className="text--center">A friend of <a href="https://www.rustacean.net/">Ferris</a>, Terres the hermit crab is a member of the Rustacean family.</p>
            <div className="text--center padding-horiz--md">
              <img className={styles.mascot} src="/SeaORM/img/Terres.png"/>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
