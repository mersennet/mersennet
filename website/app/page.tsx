import styles from './page.module.css';
import { Nav } from './components/Nav';
import { Hero } from './components/Hero';
import { Ticker } from './components/Ticker';
import { StatBand } from './components/StatBand';
import { Pillars } from './components/Pillars';
import { HowItWorks } from './components/HowItWorks';
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
        <div style={{ padding: '3.5rem 0 0' }}>
          <StatBand />
        </div>
        <Pillars />
        <HowItWorks />
        <SelectiveDisclosure />
        <Verifiability />
        <Ecosystem />
        <div style={{ padding: '4.5rem 0 5.5rem' }}>
          <CtaBand />
        </div>
      </main>
      <Footer />

      <div className={styles.scan} aria-hidden="true" />
    </div>
  );
}
