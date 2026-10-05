/** Small inline icons of the PDF viewer toolbar. They carry no name of their own. */
import type { ReactNode } from "react";

function Icon({ children }: { children: ReactNode }) {
  return (
    <svg
      className="pdf-icon"
      width="14"
      height="14"
      viewBox="0 0 14 14"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export const ChevronLeftIcon = () => (
  <Icon>
    <path d="M8.5 3 4.5 7l4 4" />
  </Icon>
);

export const ChevronRightIcon = () => (
  <Icon>
    <path d="m5.5 3 4 4-4 4" />
  </Icon>
);

export const ChevronUpIcon = () => (
  <Icon>
    <path d="m3 8.5 4-4 4 4" />
  </Icon>
);

export const ChevronDownIcon = () => (
  <Icon>
    <path d="m3 5.5 4 4 4-4" />
  </Icon>
);

export const MinusIcon = () => (
  <Icon>
    <path d="M3 7h8" />
  </Icon>
);

export const PlusIcon = () => (
  <Icon>
    <path d="M3 7h8M7 3v8" />
  </Icon>
);
