/** Moving to the subject before or after this one, from the header's buttons and from Left and Right;
 * a missing callback disables its button and its key. */
export interface ViewerNavigation {
  onPrevious?: () => void;
  onNext?: () => void;
}
