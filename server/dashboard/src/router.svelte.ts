/** Tiny hash router: #/review, #/history, ... No dependency needed. */

const DEFAULT_PATH = '/review';

function parse(): string {
  const h = window.location.hash.slice(1);
  return h.startsWith('/') ? h : DEFAULT_PATH;
}

class Router {
  path = $state(parse());

  constructor() {
    window.addEventListener('hashchange', () => {
      this.path = parse();
    });
    if (!window.location.hash) {
      window.location.replace(`#${DEFAULT_PATH}`);
    }
  }

  go(path: string): void {
    window.location.hash = `#${path}`;
  }
}

export const router = new Router();
