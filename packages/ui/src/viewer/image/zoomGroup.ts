import { createContext } from "react";

import { FIT, scaleOf, zoomTo, type ImageZoom, type ZoomBounds } from "./zoom";

/** One image of a group, as it stands: its own bounds in its own view, and the scroller its position is read from. */
export interface ZoomMember {
  /** The member that leads, the one with the highest, is the one whose scale the group's zoom is told in. */
  priority: number;
  bounds: ZoomBounds;
  scroller: HTMLElement;
}

/** What a group's images draw from, made anew on every change so a render that reads it sees the change. */
export interface ZoomGroupView {
  zoom: ImageZoom;
  lead: symbol | undefined;
  /** In the leader's scale; `null` while no member has joined. */
  bounds: ZoomBounds | null;
  leadScale: number | undefined;
  /** The scale each member draws at. */
  scales: ReadonlyMap<symbol, number>;
}

/**
 * The images that zoom and scroll as one: the two sides of an image change, or a single image alone. The zoom is
 * told in the scale of the leading member, the After side of a change; every other member draws at that scale times
 * the ratio of its fitted scale to the leader's, so each stays in the same proportion to its own view, and scrolls to
 * the same fraction of its own scroll range (`syncScroll`). Two images of the same size therefore line up pixel for
 * pixel, and images of different sizes move together from edge to edge. The percentage shown,
 * actual size and the steps are the leader's.
 */
export class ZoomGroup {
  private zoom: ImageZoom = FIT;
  private readonly members = new Map<symbol, ZoomMember>();
  private readonly listeners = new Set<() => void>();
  /** Where `syncScroll` last left each member it moved, as the browser held it, so the scroll event that move causes
   * is told from one the reader made. */
  private readonly echoes = new Map<symbol, { left: number; top: number }>();
  /** Where each member's scroller was at its last scroll event, to tell which way the reader scrolled it. */
  private readonly positions = new Map<symbol, { left: number; top: number }>();
  private view: ZoomGroupView = { zoom: FIT, lead: undefined, bounds: null, leadScale: undefined, scales: new Map() };

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  /** For `useSyncExternalStore`: the current view, the same object until something changes. */
  readonly getSnapshot = (): ZoomGroupView => this.view;

  private changed(): void {
    const scales = new Map<symbol, number>();
    for (const id of this.members.keys()) scales.set(id, this.scaleOf(id) as number);
    this.view = { zoom: this.zoom, lead: this.lead(), bounds: this.bounds(), leadScale: this.leadScale(), scales };
    for (const listener of this.listeners) listener();
  }

  /** Adds, updates or (with `null`) removes a member, once it knows its image's size and its view's. A zoom the user
   * chose keeps the leader's image at the scale it was drawn at, in the new leader's terms when the leader changes,
   * and is held within the new bounds: a larger view can raise the fitted scale past it. */
  update(id: symbol, member: ZoomMember | null): void {
    const before = this.members.get(id);
    const leadBefore = this.lead();
    const leaderBefore = this.members.get(leadBefore as symbol);
    if (member === null) {
      if (!before) return;
      this.members.delete(id);
      this.echoes.delete(id);
      this.positions.delete(id);
    } else {
      this.members.set(id, member);
      if (before && before.priority === member.priority && before.bounds.fit === member.bounds.fit && before.bounds.max === member.bounds.max) return;
    }
    const leadAfter = this.lead();
    const leaderAfter = this.members.get(leadAfter as symbol);
    const bounds = this.bounds();
    if (this.zoom.mode === "scale" && bounds) {
      // Only a change of leader converts the scale; the same leader with a new fitted scale keeps its drawn size.
      const carried = leaderBefore && leaderAfter && leadBefore !== leadAfter ? (this.zoom.scale * leaderAfter.bounds.fit) / leaderBefore.bounds.fit : this.zoom.scale;
      this.zoom = zoomTo(carried, bounds);
    }
    this.changed();
  }

  /** The member whose scale the zoom is told in. */
  lead(): symbol | undefined {
    let found: [symbol, ZoomMember] | undefined;
    for (const entry of this.members) if (!found || entry[1].priority > found[1].priority) found = entry;
    return found?.[0];
  }

  /** The bounds of the zoom, in the leader's scale: from its fitted scale up to where the first member reaches its
   * own largest scale. */
  bounds(): ZoomBounds | null {
    const id = this.lead();
    const leader = id && this.members.get(id);
    if (!leader) return null;
    const { fit } = leader.bounds;
    let max = Infinity;
    for (const member of this.members.values()) max = Math.min(max, (member.bounds.max * fit) / member.bounds.fit);
    return { fit, max: Math.max(fit, max) };
  }

  /** The zoom, in the leader's scale. */
  current(): ImageZoom {
    return this.zoom;
  }

  /** The leader's scale. */
  leadScale(): number | undefined {
    const bounds = this.bounds();
    if (!bounds) return undefined;
    return scaleOf(this.zoom, bounds.fit);
  }

  /** The scale member `id` draws at; for a member not added yet, `undefined`. */
  scaleOf(id: symbol): number | undefined {
    const member = this.members.get(id);
    const leader = this.members.get(this.lead() as symbol);
    if (!member || !leader) return undefined;
    return (scaleOf(this.zoom, leader.bounds.fit) * member.bounds.fit) / leader.bounds.fit;
  }

  /** Sets the zoom, held within the bounds. */
  setZoom(zoom: ImageZoom): void {
    const bounds = this.bounds();
    const next = zoom.mode === "fit" || !bounds ? FIT : zoomTo(zoom.scale, bounds);
    if (next.mode === this.zoom.mode && (next.mode === "fit" || (this.zoom.mode === "scale" && next.scale === this.zoom.scale))) return;
    this.zoom = next;
    this.changed();
  }

  /** A member's scroller scrolled: unless it is where `syncScroll` just put it, the reader moved it, and the others
   * follow. */
  scrolled(id: symbol): void {
    const member = this.members.get(id);
    const echo = this.echoes.get(id);
    if (!member) return;
    const now = { left: member.scroller.scrollLeft, top: member.scroller.scrollTop };
    const last = this.positions.get(id);
    this.positions.set(id, now);
    if (echo && Math.abs(now.left - echo.left) < 1 && Math.abs(now.top - echo.top) < 1) return;
    // Only the way it moved: scrolling one image across leaves where the other stands up and down alone.
    this.syncScroll(id, last ? { x: now.left !== last.left, y: now.top !== last.top } : undefined);
  }

  /** Scrolls every other member, along `axes`, to the same fraction of its own scroll range as `from` stands at in
   * its: from one end to the other, each image crosses its whole range, so two images of the same size line up pixel
   * for pixel and images of different sizes still reach their edges together. Along an axis `from` cannot scroll, its
   * image is centred, so the others go to the middle of theirs: one side zoomed past its view only across still brings
   * the other, taller one to its middle. */
  syncScroll(from: symbol, axes: { x: boolean; y: boolean } = { x: true, y: true }): void {
    const source = this.members.get(from);
    this.echoes.delete(from);
    if (!source || this.members.size < 2) return;
    const u = fractionOf(source.scroller, "x");
    const v = fractionOf(source.scroller, "y");
    for (const [id, member] of this.members) {
      if (id === from) continue;
      const { scroller } = member;
      if (axes.x) {
        const left = u * (scroller.scrollWidth - scroller.clientWidth);
        if (Math.abs(scroller.scrollLeft - left) >= 1) scroller.scrollLeft = left;
      }
      if (axes.y) {
        const top = v * (scroller.scrollHeight - scroller.clientHeight);
        if (Math.abs(scroller.scrollTop - top) >= 1) scroller.scrollTop = top;
      }
      // Read back, as the browser holds it, so the scroll event this causes is known for what it is.
      this.echoes.set(id, { left: scroller.scrollLeft, top: scroller.scrollTop });
    }
  }
}

/** How far along its scroll range a scroller stands, from 0 to 1; halfway where it cannot scroll that way, its image
 * being centred there. */
function fractionOf(scroller: HTMLElement, axis: "x" | "y"): number {
  const range = axis === "x" ? scroller.scrollWidth - scroller.clientWidth : scroller.scrollHeight - scroller.clientHeight;
  if (range < 1) return 0.5;
  return Math.min(1, Math.max(0, (axis === "x" ? scroller.scrollLeft : scroller.scrollTop) / range));
}

/** The group an image joins, and how it ranks in it; an image outside any group makes a group of its own. */
export const ZoomGroupContext = createContext<{ group: ZoomGroup; priority: number } | null>(null);
