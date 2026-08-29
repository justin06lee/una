/** Tiny hash router: #/home, #/review, ... No dependency needed. */

const DEFAULT_PATH = '/home';

/** Routes that moved when the dashboard was reorganised around Home/Insights. */
const ALIASES: Record<string, string> = {
  '/history': '/home',
  '/stats': '/insights',
};

function parse(): string {
  const h = window.location.hash.slice(1);
  const path = h.startsWith('/') ? h : DEFAULT_PATH;
  return ALIASES[path] ?? path;
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
