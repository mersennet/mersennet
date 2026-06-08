import React, { useCallback, useState } from 'react';

const NETWORK_CONFIG = {
  chainId: '0x1eef',
  chainName: 'Mersennet Testnet',
  rpcUrls: ['http://46.225.30.187:8545'],
  blockExplorerUrls: ['http://46.225.30.187'],
  nativeCurrency: {
    name: 'PRIM',
    symbol: 'PRIM',
    decimals: 18,
  },
};

type Status = 'idle' | 'pending' | 'success' | 'error';

export default function AddToMetaMask(): React.ReactElement {
  const [status, setStatus] = useState<Status>('idle');

  const add = useCallback(async () => {
    const w = window as any;
    if (!w.ethereum) {
      window.open('https://metamask.io/download/', '_blank');
      return;
    }
    setStatus('pending');
    try {
      await w.ethereum.request({
        method: 'wallet_addEthereumChain',
        params: [NETWORK_CONFIG],
      });
      setStatus('success');
      setTimeout(() => setStatus('idle'), 3000);
    } catch {
      setStatus('error');
      setTimeout(() => setStatus('idle'), 3000);
    }
  }, []);

  const label =
    status === 'pending'
      ? 'Connecting...'
      : status === 'success'
        ? 'Added!'
        : status === 'error'
          ? 'Failed — try manually'
          : 'Add Mersennet to MetaMask';

  return (
    <button
      onClick={add}
      disabled={status === 'pending'}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: '0.5rem',
        padding: '0.6rem 1.25rem',
        fontSize: '0.9rem',
        fontWeight: 600,
        fontFamily: 'var(--prime-font-mono, monospace)',
        color: status === 'success' ? '#7dff9b' : '#03150a',
        background:
          status === 'success'
            ? 'rgba(125,255,155,0.12)'
            : status === 'error'
              ? '#ef4444'
              : '#7dff9b',
        border:
          status === 'success' ? '1px solid rgba(125,255,155,0.3)' : 'none',
        borderRadius: '8px',
        cursor: status === 'pending' ? 'wait' : 'pointer',
        transition: 'all 200ms ease',
      }}
    >
      <svg
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <rect x="2" y="7" width="20" height="14" rx="2" ry="2" />
        <path d="M16 7V5a4 4 0 0 0-8 0v2" />
      </svg>
      {label}
    </button>
  );
}
