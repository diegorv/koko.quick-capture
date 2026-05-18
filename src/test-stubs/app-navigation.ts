// Test stub for `$app/navigation`. Used by vitest because this repo
// configures `@sveltejs/vite-plugin-svelte` directly (not the full
// SvelteKit plugin), so the kit runtime is not available to specs.
// Components that perform navigation must either accept an injectable
// `onNavigate`-style callback or use this stub at test time.

export const goto = async (_target: string | URL): Promise<void> => {};
export const invalidate = async (): Promise<void> => {};
export const invalidateAll = async (): Promise<void> => {};
export const preloadData = async (): Promise<void> => {};
export const preloadCode = async (): Promise<void> => {};
export const beforeNavigate = (): void => {};
export const afterNavigate = (): void => {};
