import type { ReactNode } from 'react';

export function Icon({ path, size = 22 }: { path: ReactNode; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {path}
    </svg>
  );
}

export function ArrowRight() {
  return <Icon path={<path d="M5 12h14M12 5l7 7-7 7" />} />;
}

export function BrandMark({ size = 28, flat = false }: { size?: number; flat?: boolean }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 96 96"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <g fill={flat ? 'currentColor' : '#7dff9b'}>
        <rect x="8" y="14" width="12" height="68" rx="6" />
        <rect x="25" y="14" width="12" height="30" rx="6" />
        <rect x="42" y="14" width="12" height="46" rx="6" />
        <rect x="59" y="14" width="12" height="30" rx="6" />
        <rect x="76" y="14" width="12" height="68" rx="6" />
      </g>
    </svg>
  );
}
