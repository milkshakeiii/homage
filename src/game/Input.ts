import { Camera } from '../renderer/Camera';

export interface BoxSelect {
  /** Screen-space corners */
  x0: number; y0: number;
  x1: number; y1: number;
}

export class Input {
  keys: Set<string> = new Set();
  mouseX = 0;
  mouseY = 0;
  canvasW = 0;
  canvasH = 0;
  mouseDown = false;
  private midDragging = false;
  private dragStartX = 0;
  private dragStartY = 0;
  private camStartX = 0;
  private camStartY = 0;
  clickWorld: [number, number] | null = null;
  rightClickWorld: [number, number] | null = null;
  shiftHeld = false;

  // Box-select state
  private leftDragStartX = 0;
  private leftDragStartY = 0;
  private leftDragging = false;
  /** Non-null on the frame box-select completes (screen coords) */
  boxSelect: BoxSelect | null = null;
  /** True while user is dragging a box — use for live preview */
  boxDragging = false;
  boxDragX0 = 0;
  boxDragY0 = 0;

  constructor(canvas: HTMLCanvasElement, private camera: Camera) {
    window.addEventListener('keydown', e => this.shiftHeld = e.shiftKey);
    window.addEventListener('keyup', e => this.shiftHeld = e.shiftKey);

    canvas.addEventListener('contextmenu', e => e.preventDefault());
    window.addEventListener('keydown', e => this.keys.add(e.key.toLowerCase()));
    window.addEventListener('keyup', e => this.keys.delete(e.key.toLowerCase()));

    canvas.addEventListener('mousedown', e => {
      if (e.button === 0) {
        this.mouseDown = true;
        this.leftDragging = true;
        this.leftDragStartX = e.clientX;
        this.leftDragStartY = e.clientY;
        this.boxDragX0 = e.clientX;
        this.boxDragY0 = e.clientY;
      }
      if (e.button === 1) {
        this.midDragging = true;
        this.dragStartX = e.clientX;
        this.dragStartY = e.clientY;
        this.camStartX = camera.targetX;
        this.camStartY = camera.targetY;
        e.preventDefault();
      }
    });

    canvas.addEventListener('mousemove', e => {
      this.mouseX = e.clientX;
      this.mouseY = e.clientY;
      this.canvasW = canvas.width;
      this.canvasH = canvas.height;
      if (this.midDragging) {
        camera.targetX = this.camStartX - (e.clientX - this.dragStartX) / camera.zoom;
        camera.targetY = this.camStartY - (e.clientY - this.dragStartY) / camera.zoom;
      }
      // Update box-drag preview state
      if (this.leftDragging) {
        const dx = e.clientX - this.leftDragStartX;
        const dy = e.clientY - this.leftDragStartY;
        this.boxDragging = Math.abs(dx) > 5 || Math.abs(dy) > 5;
      }
    });

    canvas.addEventListener('mouseup', e => {
      if (e.button === 2) {
        this.rightClickWorld = camera.screenToWorld(e.clientX, e.clientY, canvas.width, canvas.height);
        return;
      }
      if (e.button === 1) {
        this.midDragging = false;
        return;
      }
      if (e.button === 0) {
        if (this.leftDragging) {
          const dx = e.clientX - this.leftDragStartX;
          const dy = e.clientY - this.leftDragStartY;
          if (Math.abs(dx) > 5 || Math.abs(dy) > 5) {
            // Box-select
            this.boxSelect = {
              x0: this.leftDragStartX,
              y0: this.leftDragStartY,
              x1: e.clientX,
              y1: e.clientY,
            };
          } else {
            // Point click
            this.clickWorld = camera.screenToWorld(e.clientX, e.clientY, canvas.width, canvas.height);
          }
        }
        this.mouseDown = false;
        this.leftDragging = false;
        this.boxDragging = false;
      }
    });

    canvas.addEventListener('wheel', e => {
      camera.zoomBy(e.deltaY > 0 ? 0.85 : 1.18);
      e.preventDefault();
    }, { passive: false });
  }

  edgePan(): { x: number; y: number } {
    const EDGE = 20;
    const fullscreen = !!document.fullscreenElement;
    if (!fullscreen) return { x: 0, y: 0 };
    let x = 0, y = 0;
    if (this.mouseX < EDGE) x = -1;
    else if (this.mouseX > this.canvasW - EDGE) x = 1;
    if (this.mouseY < EDGE) y = -1;
    else if (this.mouseY > this.canvasH - EDGE) y = 1;
    return { x, y };
  }

  /** Convert screen-space box to world-space min/max */
  boxToWorld(box: BoxSelect): { minX: number; minY: number; maxX: number; maxY: number } {
    const [ax, ay] = this.camera.screenToWorld(box.x0, box.y0, this.canvasW, this.canvasH);
    const [bx, by] = this.camera.screenToWorld(box.x1, box.y1, this.canvasW, this.canvasH);
    return {
      minX: Math.min(ax, bx), minY: Math.min(ay, by),
      maxX: Math.max(ax, bx), maxY: Math.max(ay, by),
    };
  }

  isDown(key: string): boolean {
    return this.keys.has(key);
  }

  consumeClick(): [number, number] | null {
    const c = this.clickWorld;
    this.clickWorld = null;
    return c;
  }

  consumeRightClick(): [number, number] | null {
    const c = this.rightClickWorld;
    this.rightClickWorld = null;
    return c;
  }

  consumeBoxSelect(): BoxSelect | null {
    const b = this.boxSelect;
    this.boxSelect = null;
    return b;
  }
}
