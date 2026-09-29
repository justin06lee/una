/**
 * Inside una's history window, which sets `window.__UNA_APP__` before the
 * page loads. There the page is the app's main window: it makes room for the
 * macOS traffic lights (the title bar is an overlay) and offers the app's own
 * settings, which live in a window of their own.
 */

const flag = (window as { __UNA_APP__?: { platform?: string } }).__UNA_APP__;

export const inApp = !!flag;
export const inMacApp = flag?.platform === 'macos';

/** Opening this URL makes the history window show the settings window instead. */
export const APP_SETTINGS_URL = 'una://settings';

export function openAppSettings(): void {
  window.location.href = APP_SETTINGS_URL;
}
