export class LogicalSize {
  constructor(
    public width: number,
    public height: number,
  ) {}
}
export class PhysicalSize {
  constructor(
    public width: number,
    public height: number,
  ) {}
}

const win = {
  setResizable: async () => {},
  setMaximizable: async () => {},
  setSize: async () => {},
  isMaximized: async () => false,
  unmaximize: async () => {},
  maximize: async () => {},
  setFullscreen: async () => {},
  isFullscreen: async () => false,
  outerSize: async () => ({ width: 1600, height: 1000 }),
  innerSize: async () => ({ width: 1600, height: 1000 }),
  onResized: async () => () => {},
  onMoved: async () => () => {},
  scaleFactor: async () => 1,
  center: async () => {},
  setTitle: async () => {},
};

export const getCurrentWindow = () => win;
export const availableMonitors = async () => [
  {
    name: "primary",
    size: { width: 2560, height: 1440 },
    position: { x: 0, y: 0 },
    workArea: { position: { x: 0, y: 0 }, size: { width: 2560, height: 1400 } },
    scaleFactor: 1,
  },
];
export const currentMonitor = async () => (await availableMonitors())[0];
