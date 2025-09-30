import React from 'react';
import clsx from 'clsx';
import styles from './HomepageFeatures.module.css';
import { MdCloud, MdCheckCircle, MdFlashOn, MdFlight } from "react-icons/md";
import ReactMarkdown from 'react-markdown';
import Translate from '@docusaurus/Translate';

const FeatureList = [
  {
    title: <Translate description="The features title">Working Mode</Translate>,
    icon: <MdFlashOn size={26} />,
    description: `
- standalone process w/HTTP APIs
- Rust crate that can be easily integrated with other applications
    `,
  },
  {
    title: 'Configurable underlying Cryptographic Module',
    icon: <MdFlight size={26} />,
    description: `
- OpenSSL library
- Tongsuo library
- native Rust crypto libraries
`,
  },
  {
    title: 'API',
    icon: <MdCheckCircle size={26} />,
    description: `RESTful API, compatible with Hashicorp Vault`,
  },
  {
    title: 'Authentication & Authorization',
    icon: <MdCloud size={26} />,
    description: `
- X.509 certificate
- username/password
- basic ACL`,
  },
];

function Feature({icon, title, description}) {
  return (
    <div className={clsx('col col--6')}>
      <div style={{ paddingBottom: '20px', paddingTop: '20px' }}>
        <div style={{ display: 'flex' }}>
          <div style={{ paddingRight: '22px' }}>{icon}</div>
          <h3 style={{ fontSize: '20px', color: 'var(--ifm-color-primary)' }}>{title}</h3>
        </div>
        <ReactMarkdown>{description}</ReactMarkdown>
      </div>
    </div>
  );
}

export default function HomepageFeatures() {
  return (
    <section className={clsx('home-section', 'home-section-alt', styles.features)}>
      <div className="container">
        <div className="row">
          <div className="col col--11 col--offset-1">
            <div className="row">
              {FeatureList.map((props, idx) => (
                <Feature key={idx} {...props} />
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
