export const SHIP_VS = `
  attribute vec2 aPos;
  uniform mat3 uView;
  void main() {
    vec3 p = uView * vec3(aPos, 1.0);
    gl_Position = vec4(p.xy, 0.0, 1.0);
  }
`;

export const SHIP_FS = `
  precision mediump float;
  uniform vec4 uColor;
  void main() {
    gl_FragColor = uColor;
  }
`;

export const LINE_VS = `
  attribute vec2 aPos;
  attribute float aDist;
  uniform mat3 uView;
  varying float vDist;
  void main() {
    vec3 p = uView * vec3(aPos, 1.0);
    gl_Position = vec4(p.xy, 0.0, 1.0);
    vDist = aDist;
  }
`;

export const LINE_FS = `
  precision mediump float;
  uniform vec4 uColor;
  varying float vDist;
  void main() {
    float d = abs(vDist);
    float alpha = 1.0 - smoothstep(0.45, 1.0, d);
    gl_FragColor = vec4(uColor.rgb, uColor.a * alpha);
  }
`;

export const QUAD_VS = `
  attribute vec2 aPos;
  varying vec2 vUV;
  void main() {
    vUV = aPos * 0.5 + 0.5;
    gl_Position = vec4(aPos, 0.0, 1.0);
  }
`;

export const BLUR_FS = `
  precision mediump float;
  varying vec2 vUV;
  uniform sampler2D uTex;
  uniform vec2 uDir;
  void main() {
    vec4 sum = vec4(0.0);
    float weights[5];
    weights[0] = 0.227027;
    weights[1] = 0.194596;
    weights[2] = 0.121622;
    weights[3] = 0.054054;
    weights[4] = 0.016216;
    sum += texture2D(uTex, vUV) * weights[0];
    for (int i = 1; i < 5; i++) {
      vec2 off = uDir * float(i) * 2.0;
      sum += texture2D(uTex, vUV + off) * weights[i];
      sum += texture2D(uTex, vUV - off) * weights[i];
    }
    gl_FragColor = sum;
  }
`;

export const COMPOSITE_FS = `
  precision mediump float;
  varying vec2 vUV;
  uniform sampler2D uSharp;
  uniform sampler2D uBloom;
  uniform float uBloomStrength;
  void main() {
    vec4 sharp = texture2D(uSharp, vUV);
    vec4 bloom = texture2D(uBloom, vUV);
    gl_FragColor = sharp + bloom * uBloomStrength;
  }
`;

export const STARS_FS = `
  precision mediump float;
  varying vec2 vUV;
  uniform vec2 uResolution;
  uniform vec2 uCamPos;
  uniform float uZoom;
  uniform float uTime;

  float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
  }

  void main() {
    vec2 screenPos = vUV * uResolution;
    vec2 pos = screenPos + uCamPos * 0.01;
    float cellSize = 60.0;
    vec2 cell = floor(pos / cellSize);
    vec2 cellUV = fract(pos / cellSize);
    float h = hash(cell);
    float brightness = 0.0;
    if (h > 0.93) {
      vec2 starPos = vec2(hash(cell * 1.1), hash(cell * 2.3)) * 0.8 + 0.1;
      float d = length(cellUV - starPos) * cellSize;
      float starRadius = 0.6 + h * 1.0;
      float twinkle = 0.7 + 0.3 * sin(uTime * (1.0 + h * 4.0) + h * 100.0);
      brightness = smoothstep(starRadius, 0.0, d) * (0.2 + h * 0.5) * twinkle;
    }
    gl_FragColor = vec4(vec3(brightness), 1.0);
  }
`;
