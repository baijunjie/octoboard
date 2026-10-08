/**
 * Rounded glass samples inward, with magnification integrated into a monotonic displacement profile.
 * This avoids mirrored duplicates at the rim. Displacement and its derivative reach zero at the bezel.
 * R/G encode normalized x/y offsets; B weights rim blur before displacement, using sampling distance.
 * The bezel stays inside the corner radius to keep corner normals continuous.
 */
export interface LiquidGlassMapOptions {
  /** Rim width in pixels, capped at the corner radius. */
  bezel: number;
  /** Magnification at the edge, falling inward to one. */
  edgeMagnification: number;
  /** Exponent controlling how tightly magnification hugs the edge. */
  falloff: number;
  /** Sampling distance where rim weighting begins to fade, in bezel multiples. */
  rimStart: number;
  /** Distance where rim weighting reaches zero; greater than rimStart. */
  rimEnd: number;
}

function displacementProfile(
  bezel: number,
  edgeMagnification: number,
  falloff: number,
  samples = 256,
) {
  const profile = new Array<number>(samples);
  const step = bezel / (samples - 1);
  let accumulated = 0;
  profile[samples - 1] = 0;
  for (let i = samples - 2; i >= 0; i--) {
    const x = (i + 0.5) / (samples - 1);
    const magnification = 1 + (edgeMagnification - 1) * (1 - x) ** falloff;
    accumulated += (1 - 1 / magnification) * step;
    profile[i] = accumulated;
  }
  return profile;
}

function sampleProfile(profile: number[], position: number) {
  const scaled = position * (profile.length - 1);
  const index = Math.floor(scaled);
  const fraction = scaled - index;
  const next = profile[Math.min(index + 1, profile.length - 1)]!;
  return profile[index]! * (1 - fraction) + next * fraction;
}

function smoothstep(edge0: number, edge1: number, value: number) {
  if (edge1 <= edge0) return value < edge0 ? 0 : 1;
  const x = Math.min(Math.max((value - edge0) / (edge1 - edge0), 0), 1);
  return x * x * (3 - 2 * x);
}

export function liquidGlassMap(
  width: number,
  height: number,
  radius: number,
  options: LiquidGlassMapOptions,
) {
  const halfW = width / 2;
  const halfH = height / 2;
  const r = Math.min(radius, halfW, halfH);
  const bezel = Math.max(1, Math.min(options.bezel, r));
  const profile = displacementProfile(
    bezel,
    options.edgeMagnification,
    options.falloff,
  );
  const maxDisplacement = profile[0]!;
  const rimEnd = Math.min(options.rimEnd * bezel, halfW, halfH);
  const rimStart = Math.min(options.rimStart * bezel, rimEnd);
  const data = new Uint8ClampedArray(width * height * 4);

  for (let py = 0; py < height; py++) {
    for (let px = 0; px < width; px++) {
      const cx = px + 0.5 - halfW;
      const cy = py + 0.5 - halfH;
      const qx = Math.abs(cx) - (halfW - r);
      const qy = Math.abs(cy) - (halfH - r);
      const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
      const distanceToEdge = r - outside - Math.min(Math.max(qx, qy), 0);

      let dx = 0;
      let dy = 0;
      if (distanceToEdge > 0 && distanceToEdge < bezel) {
        let nx: number;
        let ny: number;
        if (qx > 0 && qy > 0) {
          nx = qx / outside;
          ny = qy / outside;
        } else if (qx > qy) {
          nx = 1;
          ny = 0;
        } else {
          nx = 0;
          ny = 1;
        }
        const magnitude =
          sampleProfile(profile, distanceToEdge / bezel) / maxDisplacement;
        dx = -nx * (Math.sign(cx) || 1) * magnitude;
        dy = -ny * (Math.sign(cy) || 1) * magnitude;
      }

      const offset = (py * width + px) * 4;
      data[offset] = 128 + dx * 127;
      data[offset + 1] = 128 + dy * 127;
      data[offset + 2] =
        (1 - smoothstep(rimStart, rimEnd, distanceToEdge)) * 255;
      data[offset + 3] = 255;
    }
  }
  // SVG displacement samples at scale * (channel - 0.5), so scale spans both offset directions.
  return { data, scale: maxDisplacement * 2 };
}
