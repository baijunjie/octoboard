# 03a Match the HeroUI v2 Alert background reference

> Goal: use HeroUI v2 Alert as the reference for status background fills.
> Completion criteria: The reference is applied to the shared Alert background in both themes, all other current
> HeroUI parts and styles remain original, and actual appearance, contrast and affected interactions are verified.

Status: **accepted; implementation not started**. Work belongs to the UI audit integration branch, after the
main-history reconstruction. It does not modify the frozen reconstructed-main candidate.

- [ ] Follow the user's [HeroUI v2 Alert reference](https://v2.heroui.com/docs/components/alert), whose default
  variant is `flat`. Verify the official published theme source and map the current statuses to its background
  colors in light and dark; do not import v2 components or change the project's component-library version.
- [ ] Keep title status color, description color, indicator, typography, spacing, radius, shadow and structure
  at their current HeroUI defaults. Only the shared Alert background is customized.
- [ ] Inspect all five current statuses and the actual Work quota warning in both themes, including Chinese
  and RTL. Measure the unchanged foregrounds against the reference backgrounds on their real surrounding surfaces.
  If a direct reference fill conflicts with the unchanged foreground or contrast requirement, present the measured
  result and concrete alternatives before deciding on a different fill or an exception.
- [ ] Use the existing shared contract tests, appropriate gallery range, static checks and independent review.
  Keep new captures and measurements outside Git. Consolidate the final rule into the existing standard, then
  close/delete this file; only the complete plan is integrated into main.
