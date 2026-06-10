'use client';

import { useState } from 'react';
import styles from '../page.module.css';

type Token = { t: string; c?: 'k' | 's' | 'f' | 'c' | 'n' };
type CodeLine = Token[];

// Hand-tokenized snippets — real APIs from the Mersennet repo, no fake calls.
const SOLIDITY: CodeLine[] = [
  [{ t: 'import', c: 'k' }, { t: ' {IPrimeOrders} ' }, { t: 'from', c: 'k' }, { t: ' "mersennet/IPrimeOrders.sol"', c: 's' }, { t: ';' }],
  [],
  [{ t: 'contract', c: 'k' }, { t: ' ' }, { t: 'MarketMaker', c: 'f' }, { t: ' {' }],
  [{ t: '    ' }, { t: 'IPrimeOrders' }, { t: ' constant', c: 'k' }, { t: ' BOOK = IPrimeOrders(' }, { t: 'address', c: 'k' }, { t: '(' }, { t: '0x0100', c: 'n' }, { t: '));' }],
  [],
  [{ t: '    ' }, { t: '// Atomic: place both sides + read fills in ONE transaction.', c: 'c' }],
  [{ t: '    ' }, { t: 'function', c: 'k' }, { t: ' ' }, { t: 'quote', c: 'f' }, { t: '(' }, { t: 'uint64', c: 'k' }, { t: ' mkt, ' }, { t: 'uint256', c: 'k' }, { t: ' bid, ' }, { t: 'uint256', c: 'k' }, { t: ' ask, ' }, { t: 'uint256', c: 'k' }, { t: ' size) ' }, { t: 'external', c: 'k' }, { t: ' {' }],
  [{ t: '        (, ' }, { t: 'uint256', c: 'k' }, { t: ' hitBid,) = BOOK.' }, { t: 'placeOrder', c: 'f' }, { t: '(mkt, ' }, { t: 'true', c: 'n' }, { t: ',  bid, size, ' }, { t: '0', c: 'n' }, { t: ');' }],
  [{ t: '        (, ' }, { t: 'uint256', c: 'k' }, { t: ' hitAsk,) = BOOK.' }, { t: 'placeOrder', c: 'f' }, { t: '(mkt, ' }, { t: 'false', c: 'n' }, { t: ', ask, size, ' }, { t: '0', c: 'n' }, { t: ');' }],
  [{ t: '    }' }],
  [{ t: '}' }],
];

const TYPESCRIPT: CodeLine[] = [
  [{ t: 'import', c: 'k' }, { t: ' { PrimeProvider, ShieldedClient, ViewingKeyHelpers } ' }, { t: 'from', c: 'k' }, { t: " '@mersennet/sdk'", c: 's' }, { t: ';' }],
  [],
  [{ t: 'const', c: 'k' }, { t: ' provider = ' }, { t: 'new', c: 'k' }, { t: ' ' }, { t: 'PrimeProvider', c: 'f' }, { t: '(' }, { t: "'http://46.225.30.187:8545'", c: 's' }, { t: ');' }],
  [{ t: 'const', c: 'k' }, { t: ' vk = ViewingKeyHelpers.' }, { t: 'fromSeed', c: 'f' }, { t: '(' }, { t: "'my recovery phrase'", c: 's' }, { t: ');' }],
  [{ t: 'const', c: 'k' }, { t: ' wallet = ' }, { t: 'new', c: 'k' }, { t: ' ' }, { t: 'ShieldedClient', c: 'f' }, { t: '({ provider, viewingKey: vk });' }],
  [],
  [{ t: '// Margin is proven in zero knowledge — the order carries no identity.', c: 'c' }],
  [{ t: 'await', c: 'k' }, { t: ' wallet.' }, { t: 'placeOrder', c: 'f' }, { t: '({ marketId: ' }, { t: '1n', c: 'n' }, { t: ', side: ' }, { t: "'buy'", c: 's' }, { t: ', price: ' }, { t: '1043n', c: 'n' }, { t: ', size: ' }, { t: '40n', c: 'n' }, { t: ' });' }],
  [],
  [{ t: '// Balances are reconstructed locally from notes only you can decrypt.', c: 'c' }],
  [{ t: 'const', c: 'k' }, { t: ' { perAsset } = ' }, { t: 'await', c: 'k' }, { t: ' wallet.' }, { t: 'getBalance', c: 'f' }, { t: '();' }],
];

const CURL: CodeLine[] = [
  [{ t: '# Standard JSON-RPC — eth_* plus the prime_* shielded namespace.', c: 'c' }],
  [{ t: 'curl', c: 'f' }, { t: ' -s http://46.225.30.187:8545 -X POST \\' }],
  [{ t: "  -H 'content-type: application/json' \\" }],
  [{ t: "  -d '", c: 's' }, { t: '{"jsonrpc":"2.0","id":1,', c: 's' }],
  [{ t: '       "method":"prime_getShieldedBalance",', c: 's' }],
  [{ t: '       "params":["vk1q8z…ke2v"]}\'', c: 's' }],
  [],
  [{ t: '# → {"result":{"perAsset":{"MRSN":"1250…000"},"noteCount":3}}', c: 'c' }],
];

const TABS = [
  { id: 'solidity', label: 'Solidity', file: 'MarketMaker.sol', code: SOLIDITY },
  { id: 'typescript', label: 'TypeScript', file: 'wallet.ts', code: TYPESCRIPT },
  { id: 'rpc', label: 'JSON-RPC', file: 'rpc.sh', code: CURL },
] as const;

export function CodeTabs() {
  const [active, setActive] = useState<(typeof TABS)[number]['id']>('solidity');
  const tab = TABS.find((t) => t.id === active)!;

  return (
    <div className={`${styles.hud} ${styles.code}`}>
      <div className={styles.codeHead} role="tablist" aria-label="Code examples">
        {TABS.map((t) => (
          <button
            key={t.id}
            role="tab"
            aria-selected={active === t.id}
            className={`${styles.codeTab} ${active === t.id ? styles.codeTabOn : ''}`}
            onClick={() => setActive(t.id)}
          >
            {t.label}
          </button>
        ))}
        <span className={styles.codeFile}>{tab.file}</span>
      </div>
      <pre className={styles.codeBody}>
        <code>
          {tab.code.map((line, i) => (
            <span key={`${tab.id}-${i}`} className={styles.codeLine}>
              <span className={styles.codeNum} aria-hidden="true">
                {String(i + 1).padStart(2, '0')}
              </span>
              {line.length === 0
                ? ' '
                : line.map((tok, j) => (
                    <span key={j} data-tok={tok.c ?? 'p'}>
                      {tok.t}
                    </span>
                  ))}
            </span>
          ))}
        </code>
      </pre>
    </div>
  );
}
