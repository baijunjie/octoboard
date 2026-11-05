// Git exports repository locations into hooks. They override cwd in the verification's temporary
// repositories, so verification must start without them. Keep configuration injection intact:
// GIT_CONFIG_COUNT and GIT_CONFIG_PARAMETERS may carry the caller's hooks and other policies.
const REPOSITORY_LOCATIONS = [
  "GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY",
  "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_SHALLOW_FILE", "GIT_GRAFT_FILE", "GIT_CONFIG",
  "GIT_PREFIX", "GIT_IMPLICIT_WORK_TREE", "GIT_INTERNAL_SUPER_PREFIX", "GIT_QUARANTINE_PATH",
];

export function withoutRepositoryEnvironment(env = process.env) {
  const clean = { ...env };
  for (const key of REPOSITORY_LOCATIONS) delete clean[key];
  return clean;
}

export function assertRepositoryEnvironmentCleared(env = process.env) {
  const present = REPOSITORY_LOCATIONS.filter((key) => env[key] !== undefined);
  if (present.length > 0) {
    throw new Error(`Refusing to run verification with repository-local Git variables: ${present.join(", ")}. Run from the working tree with those locations cleared; the pre-push hook clears them for its verification child.`);
  }
}
