// 线性图标，统一 16px / 1.6 描边，颜色跟随 currentColor
const paths = {
  play: <path d="M5 3.5v9l7.5-4.5z" fill="currentColor" stroke="none" />,
  stop: <rect x="4.5" y="4.5" width="7" height="7" rx="1.5" fill="currentColor" stroke="none" />,
  plus: <path d="M8 3.5v9M3.5 8h9" />,
  moon: <path d="M12.5 9.8A5 5 0 0 1 6.2 3.5a5 5 0 1 0 6.3 6.3z" />,
  pin: <path d="M6 2.5h4M7 2.5v3.5L4.5 8.5h7L9 6V2.5M8 8.5v5" />,
  chevronUp: <path d="M4 10l4-4 4 4" />,
  chevronDown: <path d="M4 6l4 4 4-4" />,
  chevronLeft: <path d="M10 4L6 8l4 4" />,
  chevronRight: <path d="M6 4l4 4-4 4" />,
  minus: <path d="M4 8h8" />,
  close: <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />,
  calendar: (
    <>
      <rect x="2.5" y="3.5" width="11" height="10" rx="2" />
      <path d="M2.5 6.5h11M5.5 2v3M10.5 2v3" />
    </>
  ),
  trash: <path d="M3.5 4.5h9M6.5 4.5V3h3v1.5M5 4.5l.5 8.5h5l.5-8.5" />,
  folder: <path d="M2.5 4.5a1 1 0 0 1 1-1h3l1.5 1.5h4.5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z" />,
  image: (
    <>
      <rect x="2.5" y="3" width="11" height="10" rx="2" />
      <circle cx="6" cy="6.5" r="1.2" />
      <path d="M3 12l3.5-3.5 2.5 2.5 1.5-1.5L13 12" />
    </>
  ),
  check: <path d="M3.5 8.5l3 3 6-7" />,
  bolt: <path d="M9 1.8L3.8 9h4l-1 5.2L12.2 7h-4z" />,
  chart: <path d="M2.5 13.5h11M4.5 11V8M8 11V4.5M11.5 11V6.5" />,
  clock: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M8 5v3.2l2 1.3" />
    </>
  ),
  gear: (
    <>
      <circle cx="8" cy="8" r="2" />
      <path d="M8 1.8v1.6M8 12.6v1.6M1.8 8h1.6M12.6 8h1.6M3.6 3.6l1.1 1.1M11.3 11.3l1.1 1.1M3.6 12.4l1.1-1.1M11.3 4.7l1.1-1.1" />
    </>
  ),
};

export type IconName = keyof typeof paths;

export function Icon({ name, size = 14 }: { name: IconName; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {paths[name]}
    </svg>
  );
}
