// Shared by the tests that swipe: a touch event with the finger lists a browser would fill in, which
// jsdom's own `TouchEvent` does not.

/** One finger, as the lists of a touch event hold it. */
export interface FakeTouch {
  identifier: number;
  clientX: number;
  clientY: number;
}

/** A touch event of `type` that bubbles, with `touches` (every finger on screen) and `changedTouches`
 * (the ones this event is about). */
export function touchEvent(type: string, touches: FakeTouch[], changedTouches: FakeTouch[]): Event {
  const event = new Event(type, { bubbles: true });
  Object.defineProperties(event, { touches: { value: touches }, changedTouches: { value: changedTouches } });
  return event;
}
