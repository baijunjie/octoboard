let lastMoment = 0;

/**
 * A moment on the Git mode's own clock, later than every one read before it: when a request was
 * sent, a reply taken or a session selected, so that one can be told to have come after another.
 * `performance.now()` cannot tell them apart: WebKit rounds it to the millisecond, so a request sent
 * from the same handler as what it has to come after reads the same time.
 */
export function gitMoment(): number {
  lastMoment += 1;
  return lastMoment;
}
