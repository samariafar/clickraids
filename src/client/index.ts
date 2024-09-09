import './styles/main.scss';
import { gameBySlug } from './games';
import { renderHome } from './home';
import { renderGame } from './game';

const root = document.getElementById('root') as HTMLElement;

let cleanup: (() => void) | null = null;

function currentSlug(): string {
  return window.location.pathname.replace(/^\/+|\/+$/g, '');
}

function navigate(slug: string): void {
  cleanup?.();
  cleanup = null;

  if (slug === '') {
    renderHome(root, navigate);
  } else {
    const game = gameBySlug(slug);
    if (game) {
      cleanup = renderGame(root, game);
    } else {
      document.title = 'Not Found · ClickRaids';
      root.innerHTML = '<main class="not-found"><h1>404</h1><p>Not Found</p></main>';
    }
  }
}

window.addEventListener('popstate', () => navigate(currentSlug()));

navigate(currentSlug());
