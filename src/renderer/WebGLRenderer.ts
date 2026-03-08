import { Color4 } from '../utils/Color';
import { Camera } from './Camera';
import {
  SHIP_VS, SHIP_FS, LINE_VS, LINE_FS,
  QUAD_VS, BLUR_FS, COMPOSITE_FS, STARS_FS,
} from './Shaders';

interface GLProgram extends WebGLProgram {
  uniforms: Record<string, WebGLUniformLocation>;
}

interface Framebuffer {
  fb: WebGLFramebuffer;
  tex: WebGLTexture;
  w: number;
  h: number;
}

export interface ShipPartBuffers {
  fillBuf: WebGLBuffer;
  fillCount: number;
  strokeBuf: WebGLBuffer;
  edges: Float32Array;
  edgeCount: number;
  stroke: Color4;
  fill: Color4;
  width: number;
}

const MAX_DYN_VERTS = 20000;

export class WebGLRenderer {
  gl: WebGLRenderingContext;
  canvas: HTMLCanvasElement;
  W = 0;
  H = 0;

  // Programs
  private shipProg!: GLProgram;
  private lineProg!: GLProgram;
  private blurProg!: GLProgram;
  private compositeProg!: GLProgram;
  private starsProg!: GLProgram;

  // Buffers
  private quadBuf!: WebGLBuffer;

  // Framebuffers
  private fbSharp!: Framebuffer;
  private fbBlurA!: Framebuffer;
  private fbBlurB!: Framebuffer;

  // Dynamic geometry (fills, circles)
  private dynBuf!: WebGLBuffer;
  private dynVerts = new Float32Array(MAX_DYN_VERTS * 2);
  private dynCount = 0;

  // Dynamic line geometry (SDF AA)
  private dynLineBuf!: WebGLBuffer;
  private dynLineVerts = new Float32Array(MAX_DYN_VERTS * 3);
  private dynLineCount = 0;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    const gl = canvas.getContext('webgl', { antialias: true, alpha: false });
    if (!gl) throw new Error('WebGL not supported');
    this.gl = gl;
    this.resize();
    this.initShaders();
    this.initBuffers();
    this.setupFramebuffers();
  }

  resize() {
    this.W = this.canvas.width = window.innerWidth;
    this.H = this.canvas.height = window.innerHeight;
    if (this.fbSharp) this.setupFramebuffers();
  }

  // === Shader compilation ===

  private compileShader(src: string, type: number): WebGLShader {
    const gl = this.gl;
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
      throw new Error('Shader error: ' + gl.getShaderInfoLog(s));
    }
    return s;
  }

  private createProgram(vs: string, fs: string, attribs: string[]): GLProgram {
    const gl = this.gl;
    const p = gl.createProgram()! as GLProgram;
    gl.attachShader(p, this.compileShader(vs, gl.VERTEX_SHADER));
    gl.attachShader(p, this.compileShader(fs, gl.FRAGMENT_SHADER));
    for (let i = 0; i < attribs.length; i++) gl.bindAttribLocation(p, i, attribs[i]);
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
      throw new Error('Program error: ' + gl.getProgramInfoLog(p));
    }
    p.uniforms = {};
    const n = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS);
    for (let i = 0; i < n; i++) {
      const info = gl.getActiveUniform(p, i)!;
      p.uniforms[info.name] = gl.getUniformLocation(p, info.name)!;
    }
    return p;
  }

  private initShaders() {
    this.shipProg = this.createProgram(SHIP_VS, SHIP_FS, ['aPos']);
    this.lineProg = this.createProgram(LINE_VS, LINE_FS, ['aPos', 'aDist']);
    this.blurProg = this.createProgram(QUAD_VS, BLUR_FS, ['aPos']);
    this.compositeProg = this.createProgram(QUAD_VS, COMPOSITE_FS, ['aPos']);
    this.starsProg = this.createProgram(QUAD_VS, STARS_FS, ['aPos']);
  }

  // === Buffers ===

  private initBuffers() {
    const gl = this.gl;
    this.quadBuf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.quadBuf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);

    this.dynBuf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.dynBuf);
    gl.bufferData(gl.ARRAY_BUFFER, MAX_DYN_VERTS * 8, gl.DYNAMIC_DRAW);

    this.dynLineBuf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.dynLineBuf);
    gl.bufferData(gl.ARRAY_BUFFER, MAX_DYN_VERTS * 12, gl.DYNAMIC_DRAW);
  }

  private createFB(w: number, h: number): Framebuffer {
    const gl = this.gl;
    const tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    const fb = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    return { fb, tex, w, h };
  }

  private setupFramebuffers() {
    const gl = this.gl;
    this.fbSharp = this.createFB(this.W, this.H);
    this.fbBlurA = this.createFB(Math.floor(this.W / 2), Math.floor(this.H / 2));
    this.fbBlurB = this.createFB(Math.floor(this.W / 2), Math.floor(this.H / 2));
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  // === Ship geometry helpers ===

  createShipPartBuffers(verts: number[][], stroke: Color4, fill: Color4, width: number): ShipPartBuffers {
    const gl = this.gl;
    const flat = new Float32Array(verts.length * 2);
    for (let i = 0; i < verts.length; i++) {
      flat[i * 2] = verts[i][0];
      flat[i * 2 + 1] = verts[i][1];
    }
    const fillBuf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, fillBuf);
    gl.bufferData(gl.ARRAY_BUFFER, flat, gl.STATIC_DRAW);

    const n = verts.length;
    const edgeData = new Float32Array(n * 4);
    for (let i = 0; i < n; i++) {
      edgeData[i * 4] = verts[i][0];
      edgeData[i * 4 + 1] = verts[i][1];
      edgeData[i * 4 + 2] = verts[(i + 1) % n][0];
      edgeData[i * 4 + 3] = verts[(i + 1) % n][1];
    }

    const strokeBuf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, strokeBuf);
    gl.bufferData(gl.ARRAY_BUFFER, n * 18 * 4, gl.DYNAMIC_DRAW);

    return { fillBuf, fillCount: verts.length, strokeBuf, edges: edgeData, edgeCount: n, stroke, fill, width };
  }

  private generateLineQuads(edges: Float32Array, edgeCount: number, width: number, zoom: number): Float32Array {
    const halfW = (width * 1.5 * 1.8) / zoom * 0.5;
    const data = new Float32Array(edgeCount * 18);
    for (let i = 0; i < edgeCount; i++) {
      const ax = edges[i * 4], ay = edges[i * 4 + 1];
      const bx = edges[i * 4 + 2], by = edges[i * 4 + 3];
      const dx = bx - ax, dy = by - ay;
      const len = Math.sqrt(dx * dx + dy * dy) || 1;
      const nx = (-dy / len) * halfW, ny = (dx / len) * halfW;
      const o = i * 18;
      data[o] = ax + nx; data[o + 1] = ay + ny; data[o + 2] = 1.0;
      data[o + 3] = ax - nx; data[o + 4] = ay - ny; data[o + 5] = -1.0;
      data[o + 6] = bx + nx; data[o + 7] = by + ny; data[o + 8] = 1.0;
      data[o + 9] = bx + nx; data[o + 10] = by + ny; data[o + 11] = 1.0;
      data[o + 12] = ax - nx; data[o + 13] = ay - ny; data[o + 14] = -1.0;
      data[o + 15] = bx - nx; data[o + 16] = by - ny; data[o + 17] = -1.0;
    }
    return data;
  }

  // === Dynamic geometry ===

  dynReset() { this.dynCount = 0; }

  dynCircle(cx: number, cy: number, r: number, segs: number) {
    if (this.dynCount + segs * 3 > MAX_DYN_VERTS) return;
    for (let i = 0; i < segs; i++) {
      const a1 = (i / segs) * Math.PI * 2, a2 = ((i + 1) / segs) * Math.PI * 2;
      const o = this.dynCount * 2;
      this.dynVerts[o] = cx; this.dynVerts[o + 1] = cy;
      this.dynVerts[o + 2] = cx + Math.cos(a1) * r; this.dynVerts[o + 3] = cy + Math.sin(a1) * r;
      this.dynVerts[o + 4] = cx + Math.cos(a2) * r; this.dynVerts[o + 5] = cy + Math.sin(a2) * r;
      this.dynCount += 3;
    }
  }

  dynQuad(x1: number, y1: number, x2: number, y2: number, x3: number, y3: number, x4: number, y4: number) {
    if (this.dynCount + 6 > MAX_DYN_VERTS) return;
    const o = this.dynCount * 2;
    this.dynVerts[o] = x1; this.dynVerts[o + 1] = y1;
    this.dynVerts[o + 2] = x2; this.dynVerts[o + 3] = y2;
    this.dynVerts[o + 4] = x3; this.dynVerts[o + 5] = y3;
    this.dynVerts[o + 6] = x3; this.dynVerts[o + 7] = y3;
    this.dynVerts[o + 8] = x2; this.dynVerts[o + 9] = y2;
    this.dynVerts[o + 10] = x4; this.dynVerts[o + 11] = y4;
    this.dynCount += 6;
  }

  dynFlush(viewMatrix: Float32Array, color: Color4) {
    if (this.dynCount === 0) return;
    const gl = this.gl;
    gl.useProgram(this.shipProg);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.dynBuf);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, this.dynVerts.subarray(0, this.dynCount * 2));
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.enableVertexAttribArray(0);
    gl.uniformMatrix3fv(this.shipProg.uniforms.uView, false, viewMatrix);
    gl.uniform4fv(this.shipProg.uniforms.uColor, color);
    gl.drawArrays(gl.TRIANGLES, 0, this.dynCount);
    this.dynCount = 0;
  }

  dynLine(ax: number, ay: number, bx: number, by: number, hw: number) {
    if (this.dynLineCount + 6 > MAX_DYN_VERTS) return;
    const dx = bx - ax, dy = by - ay, len = Math.sqrt(dx * dx + dy * dy) || 1;
    const nx = (-dy / len) * hw, ny = (dx / len) * hw;
    const o = this.dynLineCount * 3;
    this.dynLineVerts[o] = ax + nx; this.dynLineVerts[o + 1] = ay + ny; this.dynLineVerts[o + 2] = 1.0;
    this.dynLineVerts[o + 3] = ax - nx; this.dynLineVerts[o + 4] = ay - ny; this.dynLineVerts[o + 5] = -1.0;
    this.dynLineVerts[o + 6] = bx + nx; this.dynLineVerts[o + 7] = by + ny; this.dynLineVerts[o + 8] = 1.0;
    this.dynLineVerts[o + 9] = bx + nx; this.dynLineVerts[o + 10] = by + ny; this.dynLineVerts[o + 11] = 1.0;
    this.dynLineVerts[o + 12] = ax - nx; this.dynLineVerts[o + 13] = ay - ny; this.dynLineVerts[o + 14] = -1.0;
    this.dynLineVerts[o + 15] = bx - nx; this.dynLineVerts[o + 16] = by - ny; this.dynLineVerts[o + 17] = -1.0;
    this.dynLineCount += 6;
  }

  dynLineFlush(viewMatrix: Float32Array, color: Color4) {
    if (this.dynLineCount === 0) return;
    const gl = this.gl;
    gl.useProgram(this.lineProg);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.dynLineBuf);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, this.dynLineVerts.subarray(0, this.dynLineCount * 3));
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 12, 0);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(1, 1, gl.FLOAT, false, 12, 8);
    gl.enableVertexAttribArray(1);
    gl.uniformMatrix3fv(this.lineProg.uniforms.uView, false, viewMatrix);
    gl.uniform4fv(this.lineProg.uniforms.uColor, color);
    gl.drawArrays(gl.TRIANGLES, 0, this.dynLineCount);
    this.dynLineCount = 0;
    gl.disableVertexAttribArray(1);
  }

  // Raw triangle write into dyn buffer (for fighters etc)
  dynTriangles(data: Float32Array, count: number) {
    if (this.dynCount + count > MAX_DYN_VERTS) return;
    this.dynVerts.set(data.subarray(0, count * 2), this.dynCount * 2);
    this.dynCount += count;
  }

  // === Ship rendering ===

  drawShip(parts: ShipPartBuffers[], x: number, y: number, angle: number, viewMatrix: Float32Array, zoom: number) {
    const gl = this.gl;
    const cos = Math.cos(angle), sin = Math.sin(angle);
    const vm = viewMatrix;
    const m = new Float32Array([
      vm[0] * cos + vm[3] * sin,
      vm[1] * cos + vm[4] * sin,
      0,
      vm[0] * -sin + vm[3] * cos,
      vm[1] * -sin + vm[4] * cos,
      0,
      vm[0] * x + vm[3] * y + vm[6],
      vm[1] * x + vm[4] * y + vm[7],
      1,
    ]);

    for (const part of parts) {
      // Fill
      if (part.fill[3] > 0.01) {
        gl.useProgram(this.shipProg);
        gl.uniformMatrix3fv(this.shipProg.uniforms.uView, false, m);
        gl.bindBuffer(gl.ARRAY_BUFFER, part.fillBuf);
        gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
        gl.enableVertexAttribArray(0);
        gl.uniform4fv(this.shipProg.uniforms.uColor, part.fill);
        gl.drawArrays(gl.TRIANGLE_FAN, 0, part.fillCount);
      }

      // Stroke (SDF AA lines)
      gl.useProgram(this.lineProg);
      gl.uniformMatrix3fv(this.lineProg.uniforms.uView, false, m);
      gl.uniform4fv(this.lineProg.uniforms.uColor, part.stroke);
      const lineData = this.generateLineQuads(part.edges, part.edgeCount, part.width, zoom);
      gl.bindBuffer(gl.ARRAY_BUFFER, part.strokeBuf);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, lineData);
      gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 12, 0);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(1, 1, gl.FLOAT, false, 12, 8);
      gl.enableVertexAttribArray(1);
      gl.drawArrays(gl.TRIANGLES, 0, part.edgeCount * 6);
      gl.disableVertexAttribArray(1);
    }
  }

  // === Full frame render ===

  beginFrame(camera: Camera, time: number) {
    const gl = this.gl;
    gl.viewport(0, 0, this.W, this.H);

    // Render to fbSharp
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbSharp.fb);
    gl.clearColor(0, 0, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);

    // Stars
    gl.useProgram(this.starsProg);
    gl.uniform2f(this.starsProg.uniforms.uResolution, this.W, this.H);
    gl.uniform2f(this.starsProg.uniforms.uCamPos, camera.x, camera.y);
    gl.uniform1f(this.starsProg.uniforms.uZoom, camera.zoom);
    gl.uniform1f(this.starsProg.uniforms.uTime, time);
    this.drawFullscreenQuad();

    // Enable blending for ships/effects
    gl.useProgram(this.shipProg);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  }

  endFrame() {
    const gl = this.gl;

    // Bloom pass
    gl.disable(gl.BLEND);

    // Horizontal blur: sharp → blurA
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbBlurA.fb);
    gl.viewport(0, 0, this.fbBlurA.w, this.fbBlurA.h);
    gl.useProgram(this.blurProg);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.fbSharp.tex);
    gl.uniform1i(this.blurProg.uniforms.uTex, 0);
    gl.uniform2f(this.blurProg.uniforms.uDir, 1.0 / this.fbBlurA.w, 0);
    this.drawFullscreenQuad();

    // Vertical blur: blurA → blurB
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbBlurB.fb);
    gl.bindTexture(gl.TEXTURE_2D, this.fbBlurA.tex);
    gl.uniform2f(this.blurProg.uniforms.uDir, 0, 1.0 / this.fbBlurA.h);
    this.drawFullscreenQuad();

    // Extra blur passes
    for (let pass = 0; pass < 2; pass++) {
      const spread = (pass + 1) * 1.5;
      gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbBlurA.fb);
      gl.bindTexture(gl.TEXTURE_2D, this.fbBlurB.tex);
      gl.uniform2f(this.blurProg.uniforms.uDir, spread / this.fbBlurA.w, 0);
      this.drawFullscreenQuad();
      gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbBlurB.fb);
      gl.bindTexture(gl.TEXTURE_2D, this.fbBlurA.tex);
      gl.uniform2f(this.blurProg.uniforms.uDir, 0, spread / this.fbBlurA.h);
      this.drawFullscreenQuad();
    }

    // Composite to screen
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, this.W, this.H);
    gl.useProgram(this.compositeProg);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.fbSharp.tex);
    gl.uniform1i(this.compositeProg.uniforms.uSharp, 0);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, this.fbBlurB.tex);
    gl.uniform1i(this.compositeProg.uniforms.uBloom, 1);
    gl.uniform1f(this.compositeProg.uniforms.uBloomStrength, 2.0);
    this.drawFullscreenQuad();
  }

  private drawFullscreenQuad() {
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.quadBuf);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.enableVertexAttribArray(0);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }
}
