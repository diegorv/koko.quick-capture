// Pure build-info helpers shared by Vite's build-time config and
// runtime tests. Kept in plain JS (with JSDoc types) so vite.config.js
// can import it directly at config-load time without a TS transpile.

/** @typedef {'stable' | 'nightly'} ReleaseChannel */

/**
 * @typedef {Object} VersionInputs
 * @property {string} pkgVersion
 * @property {string} gitHash
 * @property {string} commitCount - Only used for nightly builds.
 * @property {ReleaseChannel} channel
 */

/**
 * Stable: returns pkgVersion as-is. Nightly: appends
 * `-nightly.<count>.<sha>` so consecutive nightlies sort monotonically
 * under semver (numeric prerelease identifiers compare numerically).
 *
 * Nightly is intentionally semver-greater than the same-base stable
 * counterpart, so the auto-updater never silently downgrades. Switching
 * nightly -> stable needs the explicit `allow_downgrades` path.
 *
 * @param {VersionInputs} inputs
 * @returns {string}
 */
export function resolveVersion(inputs) {
  const { pkgVersion, gitHash, commitCount, channel } = inputs;
  if (channel === "nightly") {
    return `${pkgVersion}-nightly.${commitCount}.${gitHash}`;
  }
  return pkgVersion;
}

/**
 * Display string injected as `__BUILD_INFO__`. Nightly drops the
 * separate `(sha)` segment because the version string already ends
 * with the hash as its semver tiebreaker.
 *
 * @param {VersionInputs & { buildTime: string }} inputs
 * @returns {string}
 */
export function formatBuildInfo(inputs) {
  const version = resolveVersion(inputs);
  const shaSegment = inputs.channel === "nightly" ? "" : `(${inputs.gitHash}) `;
  return `${version} ${shaSegment}(${inputs.buildTime})`;
}

/**
 * Normalise the release channel from an env-var string. Unknown values
 * default to stable; comparison is case-insensitive.
 *
 * @param {string | undefined} value
 * @returns {ReleaseChannel}
 */
export function parseReleaseChannel(value) {
  if (value && value.toLowerCase() === "nightly") {
    return "nightly";
  }
  return "stable";
}

/**
 * Uppercased channel label used in the build-info badge.
 *
 * @param {ReleaseChannel} channel
 * @returns {string}
 */
export function channelLabel(channel) {
  return channel === "nightly" ? "NIGHTLY" : "STABLE";
}
