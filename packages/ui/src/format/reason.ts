/** A short note under a label: why something cannot be chosen, or how to make it available, or what an option
 * stands for. The state ("Limit reached") and, where there is one, the detail that goes with it ("resets in 2h"). */
export interface Reason {
  state: string;
  detail?: string;
}

/** The reason as one phrase, for a name or description read aloud: "Limit reached · resets in 2h". */
export function reasonText({ state, detail }: Reason): string {
  return detail ? `${state} · ${detail}` : state;
}
