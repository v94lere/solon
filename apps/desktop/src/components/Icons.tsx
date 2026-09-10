// Icônes des actions (traits 1.8, 16 px), toutes décoratives : le libellé est porté par aria-label.
import type { SVGProps } from "react";

const base: SVGProps<SVGSVGElement> = {
  viewBox: "0 0 24 24",
  width: 16,
  height: 16,
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round",
  strokeLinejoin: "round",
  "aria-hidden": true,
};

export const IconPlay = () => (
  <svg {...base}>
    <path d="M7 4.5v15l12-7.5Z" fill="currentColor" stroke="none" />
  </svg>
);
export const IconStop = () => (
  <svg {...base}>
    <rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor" stroke="none" />
  </svg>
);
export const IconRestart = () => (
  <svg {...base}>
    <path d="M20 12a8 8 0 1 1-2.6-5.9" />
    <path d="M20 4v5h-5" />
  </svg>
);
export const IconLogs = () => (
  <svg {...base}>
    <path d="M4 6h16M4 12h16M4 18h10" />
  </svg>
);
export const IconTrash = () => (
  <svg {...base}>
    <path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13" />
  </svg>
);
export const IconTerminal = () => (
  <svg {...base}>
    <path d="m5 7 5 5-5 5M12 17h7" />
  </svg>
);
export const IconFile = () => (
  <svg {...base}>
    <path d="M14 3H7a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V8z" />
    <path d="M14 3v5h5M9 13h6M9 17h6" />
  </svg>
);
export const IconPencil = () => (
  <svg {...base}>
    <path d="M4 20h4l10.5-10.5a2.1 2.1 0 0 0-3-3L5 17z" />
    <path d="m13.5 6.5 3 3" />
  </svg>
);
