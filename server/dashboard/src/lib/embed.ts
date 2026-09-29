/**
 * Embedded in the Una desktop window (`?embed` in the URL): the window draws
 * the navigation and the window chrome, so the dashboard shows just the page.
 * It tells the window where it is, and hands over the links the window
 * serves itself (Review and Settings are the app's own there).
 */

export const embedded = new URLSearchParams(window.location.search).has('embed');

/** Paths the Una window shows with its own views rather than this page. */
const HOST_PATHS = new Set(['/review', '/settings']);

export type HostMessage = { type: 'una:route' | 'una:navigate'; path: string };

/**
 * Pages the window asks for: `{ type: 'una:go', path }`, sent when its sidebar
 * moves here or comes back from a page of its own. Answering means showing
 * that page fresh, so `go` is called even for the page already on screen.
 */
export function listenToHost(go: (path: string) => void): () => void {
  const onMessage = (event: MessageEvent) => {
    if (event.source !== window.parent) return;
    const message = event.data as { type?: string; path?: string } | null;
    if (message?.type === 'una:go' && message.path?.startsWith('/')) go(message.path);
  };
  window.addEventListener('message', onMessage);
  return () => window.removeEventListener('message', onMessage);
}

export function tellHost(message: HostMessage): void {
  // The window's origin differs between builds (tauri://localhost, the dev
  // server), and nothing sensitive is in the message.
  if (embedded) window.parent.postMessage(message, '*');
}

/** Route in-page links to Review/Settings through the window instead. */
export function captureHostLinks(): () => void {
  const onClick = (event: MouseEvent) => {
    const link = (event.target as Element | null)?.closest?.('a[href^="#/"]');
    const path = link?.getAttribute('href')?.slice(1);
    if (!path || !HOST_PATHS.has(path)) return;
    event.preventDefault();
    tellHost({ type: 'una:navigate', path });
  };
  document.addEventListener('click', onClick, true);
  return () => document.removeEventListener('click', onClick, true);
}
