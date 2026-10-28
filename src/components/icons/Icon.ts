import { h, type FunctionalComponent } from "vue";

/** Inline stroke icons (Lucide-style geometry), rendered with `h()` — no `v-html` anywhere. */
type Shape = readonly [
  tag: "path" | "rect" | "polygon" | "circle",
  attrs: Record<string, string | number>,
];

const ICONS = {
  play: [["polygon", { points: "6 3 20 12 6 21 6 3" }]],
  pause: [
    ["rect", { x: 6, y: 4, width: 4, height: 16, rx: 1 }],
    ["rect", { x: 14, y: 4, width: 4, height: 16, rx: 1 }],
  ],
  stop: [["rect", { x: 5, y: 5, width: 14, height: 14, rx: 2 }]],
  close: [["path", { d: "M18 6 6 18M6 6l12 12" }]],
  minus: [["path", { d: "M5 12h14" }]],
  settings: [
    ["path", { d: "M21 4h-7M10 4H3M21 12h-9M8 12H3M21 20h-5M12 20H3M14 2v4M8 10v4M16 18v4" }],
  ],
  expand: [["path", { d: "M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" }]],
  shrink: [["path", { d: "M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7" }]],
  send: [["path", { d: "m22 2-7 20-4-9-9-4ZM22 2 11 13" }]],
  alert: [
    [
      "path",
      {
        d: "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3M12 9v4M12 17h.01",
      },
    ],
  ],
  retry: [["path", { d: "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8M3 3v5h5" }]],
  copy: [
    ["rect", { x: 8, y: 8, width: 14, height: 14, rx: 2 }],
    ["path", { d: "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" }],
  ],
  keyboard: [
    ["rect", { x: 2, y: 4, width: 20, height: 16, rx: 2 }],
    ["path", { d: "M6 8h.01M10 8h.01M14 8h.01M18 8h.01M8 12h.01M12 12h.01M16 12h.01M7 16h10" }],
  ],
  "follow-up": [["path", { d: "m15 10 5 5-5 5M4 4v7a4 4 0 0 0 4 4h12" }]],
  "arrow-down": [["path", { d: "M12 5v14M19 12l-7 7-7-7" }]],
  "arrow-up": [["path", { d: "M12 19V5M5 12l7-7 7 7" }]],
  trash: [
    [
      "path",
      {
        d: "M3 6h18M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2",
      },
    ],
  ],
  speaker: [
    ["polygon", { points: "11 5 6 9 2 9 2 15 6 15 11 19 11 5" }],
    ["path", { d: "M15.54 8.46a5 5 0 0 1 0 7.07M19.07 4.93a10 10 0 0 1 0 14.14" }],
  ],
  "speaker-off": [
    ["polygon", { points: "11 5 6 9 2 9 2 15 6 15 11 19 11 5" }],
    ["path", { d: "m22 9-6 6M16 9l6 6" }],
  ],
  mic: [
    ["path", { d: "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" }],
    ["path", { d: "M19 10v2a7 7 0 0 1-14 0v-2M12 19v3" }],
  ],
  "mic-off": [
    [
      "path",
      {
        d: "m2 2 20 20M18.89 13.23A7.12 7.12 0 0 0 19 12v-2M5 10v2a7 7 0 0 0 12 5M15 9.34V5a3 3 0 0 0-5.68-1.33M9 9v3a3 3 0 0 0 5.12 2.12M12 19v3",
      },
    ],
  ],
  refresh: [
    [
      "path",
      {
        d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8M21 3v5h-5M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16M8 16H3v5",
      },
    ],
  ],
  monitor: [
    ["rect", { x: 2, y: 3, width: 20, height: 14, rx: 2 }],
    ["path", { d: "M8 21h8M12 17v4" }],
  ],
} satisfies Record<string, readonly Shape[]>;

export type IconName = keyof typeof ICONS;

const Icon: FunctionalComponent<{ name: IconName; size?: number }> = (props) => {
  const size = props.size ?? 16;
  return h(
    "svg",
    {
      width: size,
      height: size,
      viewBox: "0 0 24 24",
      fill: "none",
      stroke: "currentColor",
      "stroke-width": 2,
      "stroke-linecap": "round",
      "stroke-linejoin": "round",
      "aria-hidden": "true",
      focusable: "false",
    },
    ICONS[props.name].map(([tag, attrs]) => h(tag, attrs)),
  );
};
Icon.props = ["name", "size"];

export default Icon;
