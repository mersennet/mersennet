import styles from './page.module.css';
import { Nav } from './components/Nav';
import { Hero } from './components/Hero';
import { Ticker } from './components/Ticker';
import { StatBand } from './components/StatBand';
import { Pillars } from './components/Pillars';
import { Thesis } from './components/Thesis';
import { Edge } from './components/Edge';
import { Momentum } from './components/Momentum';
import { HowItWorks } from './components/HowItWorks';
import { BuildShowcase } from './components/BuildShowcase';
import { SelectiveDisclosure } from './components/SelectiveDisclosure';
import { Verifiability } from './components/Verifiability';
import { Ecosystem } from './components/Ecosystem';
import { CtaBand } from './components/CtaBand';
import { Footer } from './components/Footer';

export default function Home() {
  return (
    <div className={styles.page}>
      <div className={styles.vignette} aria-hidden="true" />

      <Nav />
      <main>
        <Hero />
        <Ticker />
        <div className={styles.statBandWrap}>
          <StatBand />
        </div>
        <Pillars />
        <Thesis />
        <Edge />
        <HowItWorks />
        <BuildShowcase />
        <SelectiveDisclosure />
        <Verifiability />
        <Momentum />
        <Ecosystem />
        <div className={styles.ctaBandWrap}>
          <CtaBand />
        </div>
      </main>
      <Footer />

      <div className={styles.scan} aria-hidden="true" />
    </div>
  );
}
