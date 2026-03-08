import { Camera } from '../renderer/Camera';

export class Input {
  keys: Set<string> = new Set();
  mouseX = 0;
  mouseY = 0;
  mouseDown = false;
  private dragging = false;
  private dragStartX = 0;
  private dragStartY = 0;
  private camStartX = 0;
  private camStartY = 0;
  clickWorld: [number, number] | null = null; // set on click, consumed by game

  constructor(canvas: HTMLCanvasElement, camera: Camera) {
    window.addEventListener('keydown', e => this.keys.add(e.key.toLowerCase()));
    window.addEventListener('keyup', e => this.keys.delete(e.key.toLowerCase()));

    canvas.addEventListener('mousedown', e => {
      this.mouseDown = true;
      this.dragging = true;
      this.dragStartX = e.clientX;
      this.dragStartY = e.clientY;
      this.camStartX = camera.targetX;
      this.camStartY = camera.targetY;
    });

    canvas.addEventListener('mousemove', e => {
      this.mouseX = e.clientX;
      this.mouseY = e.clientY;
      if (this.dragging) {
        camera.targetX = this.camStartX - (e.clientX - this.dragStartX) / camera.zoom;
        camera.targetY = this.camStartY - (e.clientY - this.dragStartY) / camera.zoom;
      }
    });

    canvas.addEventListener('mouseup', e => {
      if (this.dragging) {
        const dx = e.clientX - this.dragStartX;
        const dy = e.clientY - this.dragStartY;
        if (Math.abs(dx) < 5 && Math.abs(dy) < 5) {
          this.clickWorld = camera.screenToWorld(e.clientX, e.clientY, canvas.width, canvas.height);
        }
      }
      this.mouseDown = false;
      this.dragging = false;
    });

    canvas.addEventListener('wheel', e => {
      camera.zoomBy(e.deltaY > 0 ? 0.85 : 1.18);
      e.preventDefault();
    }, { passive: false });
  }

  isDown(key: string): boolean {
    return this.keys.has(key);
  }

  consumeClick(): [number, number] | null {
    const c = this.clickWorld;
    this.clickWorld = null;
    return c;
  }
}
