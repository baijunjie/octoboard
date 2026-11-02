import { readFileSync } from "node:fs";

import { expect, it } from "vitest";

import { SLEEP_PATHS } from "./sleepGlyph";

/** The glyph's stroke, in the 24-unit grid's units; a letter's scale scales its stroke with it. */
const STROKE = 2;
const GRID = 24;

/** The extent of paths made of absolute `M` and relative `h`, `v` and `l` segments, the only ones the "zz" uses. */
function extent(paths: readonly string[]): { x: [number, number]; y: [number, number] } {
  const xs: number[] = [];
  const ys: number[] = [];
  for (const d of paths) {
    let x = 0;
    let y = 0;
    for (const [, command, args] of d.matchAll(/([MhvlL])([^A-Za-z]*)/g)) {
      const [a = 0, b = 0] = args.trim().split(/[\s,]+/).map(Number);
      if (command === "M" || command === "L") [x, y] = [a, b];
      else if (command === "h") x += a;
      else if (command === "v") y += a;
      else [x, y] = [x + a, y + b];
      xs.push(x);
      ys.push(y);
    }
  }
  return { x: [Math.min(...xs), Math.max(...xs)], y: [Math.min(...ys), Math.max(...ys)] };
}

it("draws the 'zz' with its ink's box centred on the grid's centre, 12 and 12", () => {
  const { x, y } = extent(SLEEP_PATHS);
  expect((x[0] + x[1]) / 2).toBe(12);
  expect((y[0] + y[1]) / 2).toBe(12);
});

type Point = [number, number];
type Pose = { tx: number; ty: number; scale: number };

/** The vertices of a path made of the segments `extent` reads. */
function vertices(d: string): Point[] {
  const points: Point[] = [];
  let x = 0;
  let y = 0;
  for (const [, command, args] of d.matchAll(/([MhvlL])([^A-Za-z]*)/g)) {
    const [a = 0, b = 0] = args.trim().split(/[\s,]+/).map(Number);
    if (command === "M" || command === "L") [x, y] = [a, b];
    else if (command === "h") x += a;
    else if (command === "v") y += a;
    else [x, y] = [x + a, y + b];
    points.push([x, y]);
  }
  return points;
}

const KEYFRAMES = readFileSync(new URL("../../style.css", import.meta.url), "utf8");

/** The poses a keyframe rule gives, by stop (as a fraction of the loop), read from `style.css` so the test and the
 * animation cannot drift apart. */
function posesOf(name: string): Array<{ at: number } & Pose> {
  const block = KEYFRAMES.match(new RegExp(`@keyframes ${name} \\{([\\s\\S]*?)\\n\\}`))![1];
  return [...block.matchAll(/([\d.]+)%\s*\{\s*transform:\s*translate\((-?[\d.]+)(?:px)?,\s*(-?[\d.]+)(?:px)?\)\s*scale\((-?[\d.]+)\)/g)].map(([, at, tx, ty, scale]) => ({ at: Number(at) / 100, tx: Number(tx), ty: Number(ty), scale: Number(scale) }));
}

/** A letter's vertices under a pose, which scales about the centre of its own box (`transform-box: fill-box`). */
function placed(d: string, { tx, ty, scale }: Pose): Point[] {
  const { x, y } = extent([d]);
  const cx = (x[0] + x[1]) / 2;
  const cy = (y[0] + y[1]) / 2;
  return vertices(d).map(([px, py]) => [cx + (px - cx) * scale + tx, cy + (py - cy) * scale + ty]);
}

const mix = (from: Pose, to: Pose, u: number): Pose => ({ tx: from.tx + (to.tx - from.tx) * u, ty: from.ty + (to.ty - from.ty) * u, scale: from.scale + (to.scale - from.scale) * u });

function inkBox(points: Point[], scale: number): { x: [number, number]; y: [number, number] } {
  const half = (STROKE * scale) / 2;
  const xs = points.map((point) => point[0]);
  const ys = points.map((point) => point[1]);
  return { x: [Math.min(...xs) - half, Math.max(...xs) + half], y: [Math.min(...ys) - half, Math.max(...ys) + half] };
}

function distanceToSegment([px, py]: Point, [ax, ay]: Point, [bx, by]: Point): number {
  const dx = bx - ax;
  const dy = by - ay;
  const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)));
  return Math.hypot(px - ax - t * dx, py - ay - t * dy);
}

/** The clear space between two stroked polylines (their centre lines' distance less both stroke radii); the Zs'
 * segments never cross, so the nearest approach is at a vertex of one of them. */
function gap(a: Point[], scaleA: number, b: Point[], scaleB: number): number {
  let nearest = Infinity;
  for (const [from, to] of [[a, b], [b, a]] as const) {
    for (const point of from) for (let index = 1; index < to.length; index++) nearest = Math.min(nearest, distanceToSegment(point, to[index - 1], to[index]));
  }
  return nearest - ((scaleA + scaleB) * STROKE) / 2;
}

const STOPS = posesOf("status-scroll");

it("has every scrolling Z wholly outside the grid at both ends of its run, so the jump back is never seen", () => {
  expect([STOPS[0].at, STOPS[STOPS.length - 1].at]).toEqual([0, 1]);
  for (const pose of [STOPS[0], STOPS[STOPS.length - 1]]) {
    const { x, y } = inkBox(placed(SLEEP_PATHS[0], pose), pose.scale);
    const outside = x[1] <= 0 || x[0] >= GRID || y[1] <= 0 || y[0] >= GRID;
    expect(outside, `x ${x} y ${y}`).toBe(true);
  }
});

it("shrinks by the same ratio in equal times, with no knee, and keeps each Z's offset in step with its size", () => {
  expect(STOPS.length).toBeGreaterThanOrEqual(21);
  const step = STOPS[1].at - STOPS[0].at;
  const ratio = STOPS[1].scale / STOPS[0].scale;
  const offsetPerScale = (STOPS[1].tx - STOPS[0].tx) / (STOPS[1].scale - STOPS[0].scale);
  for (let index = 1; index < STOPS.length; index++) {
    expect(STOPS[index].at - STOPS[index - 1].at).toBeCloseTo(step, 6);
    expect(STOPS[index].scale / STOPS[index - 1].scale).toBeCloseTo(ratio, 3);
    // The keyframes are rounded to a thousandth, which this ratio of differences magnifies.
    expect(Math.abs((STOPS[index].tx - STOPS[index - 1].tx) / (STOPS[index].scale - STOPS[index - 1].scale) - offsetPerScale)).toBeLessThan(0.7);
    expect(STOPS[index].ty).toBeCloseTo(-STOPS[index].tx, 3);
  }
});

/** The fraction of the loop a copy is ahead of the first, read from the theme entry's duration and delay. */
function phaseOf(name: string): number {
  const [, duration, delay = "0"] = KEYFRAMES.match(new RegExp(`--animate-${name}: status-scroll ([\\d.]+)s linear (?:(-?[\\d.]+)s )?infinite`))!;
  return (((-Number(delay) / Number(duration)) % 1) + 1) % 1;
}

/** The pose at `u` of the loop, between the keyframes either side of it. */
function loopPose(u: number): Pose {
  const next = Math.max(1, STOPS.findIndex((stop) => stop.at >= u));
  return mix(STOPS[next - 1], STOPS[next], (u - STOPS[next - 1].at) / (STOPS[next].at - STOPS[next - 1].at));
}

/** The share of a pose's ink box that lies inside the grid. */
function shownShare(pose: Pose): number {
  const { x, y } = inkBox(placed(SLEEP_PATHS[0], pose), pose.scale);
  const inside = Math.max(0, Math.min(x[1], GRID) - Math.max(x[0], 0)) * Math.max(0, Math.min(y[1], GRID) - Math.max(y[0], 0));
  return inside / ((x[1] - x[0]) * (y[1] - y[0]));
}

it("keeps the scrolling Zs clear of each other, two in view most of the time and clearly different in size", () => {
  const offsets = ["status-scroll", "status-scroll-second", "status-scroll-third"].map(phaseOf);
  expect(offsets.map((offset) => +offset.toFixed(2))).toEqual([0, 0.33, 0.67]);
  let nearest = Infinity;
  let closestRatio = Infinity;
  let smallest = Infinity;
  let crowded = 0;
  const steps = 300;
  for (let step = 0; step < steps; step++) {
    const poses = offsets.map((offset) => loopPose((step / steps + offset) % 1));
    for (let a = 0; a < poses.length; a++) {
      for (let b = a + 1; b < poses.length; b++) nearest = Math.min(nearest, gap(placed(SLEEP_PATHS[0], poses[a]), poses[a].scale, placed(SLEEP_PATHS[0], poses[b]), poses[b].scale));
    }
    // A Z counts as in view with half of it inside the box; one wholly outside is none.
    const inView = poses.filter((pose) => shownShare(pose) >= 0.5).map((pose) => pose.scale).sort((p, q) => q - p);
    if (inView.length < 2) crowded++;
    for (let index = 1; index < inView.length; index++) closestRatio = Math.min(closestRatio, inView[index - 1] / inView[index]);
    if (inView.length > 0) smallest = Math.min(smallest, inView[inView.length - 1]);
  }
  expect(nearest).toBeGreaterThan(2);
  expect(closestRatio).toBeGreaterThan(1.7);
  expect(smallest).toBeGreaterThan(0.29);
  expect(crowded / steps).toBeLessThan(0.1);
});
