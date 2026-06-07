import { decompress } from 'fzstd';
import { Game, gameName } from './games';

export function renderGame(root: HTMLElement, game: Game): () => void {
  document.title = `${gameName(game)} | ClickRaids — Pick Your Battle`;

  root.innerHTML = `
    <div id="checkerboard" class="game game--${game.slug}"></div>
  `;

  const container = document.getElementById('checkerboard') as HTMLElement;
  const totalRows = 1000;
  const totalCols = 1000;
  // Keep in sync with --cell-size in main.scss
  const rowHeight = 40;
  const colWidth = 40;

  const checkboxStates: boolean[] = new Array(totalRows * totalCols).fill(false);

  const spacer = document.createElement('div');
  spacer.style.width = `${totalCols * colWidth}px`;
  spacer.style.height = `${totalRows * rowHeight}px`;
  container.appendChild(spacer);

  const createRow = (i: number, startCol: number, endCol: number): HTMLDivElement => {
    const row = document.createElement('div');
    row.className = 'row';
    row.style.position = 'absolute';
    row.style.top = `${i * rowHeight}px`;
    row.style.width = '100%';

    for (let j = startCol; j <= endCol; j++) {
      const cell = document.createElement('div');
      const index = i * totalCols + j;

      cell.className = 'cell';
      cell.style.position = 'absolute';
      cell.style.left = `${j * colWidth}px`;

      const checkbox = document.createElement('input');
      checkbox.type = 'checkbox';
      checkbox.className = `checkbox ${index}`;
      checkbox.checked = checkboxStates[index];
      checkbox.addEventListener('click', () => {
        window.op('track', `${gameName(game)}_game_clicks`);
        window.op('track', 'total_game_clicks');
      });
      checkbox.addEventListener('change', () => {
        checkboxStates[index] = checkbox.checked;

        const buf = new ArrayBuffer(4);
        new DataView(buf).setUint32(0, (checkbox.checked ? 0x80000000 : 0) | (index & 0x7fffffff), false);
        websocket.send(buf);
      });
      cell.appendChild(checkbox);

      row.appendChild(cell);
    }

    return row;
  };

  let activeRows = new Map<number, HTMLDivElement>();

  const handleScroll = () => {
    const scrollTop = container.scrollTop;
    const scrollLeft = container.scrollLeft;
    const startRow = Math.floor(scrollTop / rowHeight);
    const startCol = Math.floor(scrollLeft / colWidth);
    const endRow = Math.min(startRow + Math.ceil(container.clientHeight / rowHeight), totalRows - 1);
    const endCol = Math.min(startCol + Math.ceil(container.clientWidth / colWidth), totalCols - 1);

    activeRows.forEach((row, index) => {
      if (index < startRow || index > endRow) {
        container.removeChild(row);
        activeRows.delete(index);
      }
    });

    for (let i = startRow; i <= endRow; i++) {
      const row = createRow(i, startCol, endCol);

      if (activeRows.has(i)) {
        container.removeChild(activeRows.get(i)!);
      }

      container.appendChild(row);
      activeRows.set(i, row);
    }
  };

  const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const websocket = new WebSocket(`${wsProtocol}//${window.location.host}/ws/${game.slug}`);
  websocket.binaryType = 'arraybuffer';

  let receivedSnapshot = false;

  websocket.onopen = () => {
    console.log('WebSocket connection established');
  };

  websocket.onmessage = (event: MessageEvent) => {
    if (!(event.data instanceof ArrayBuffer)) return;

    if (!receivedSnapshot) {
      const snapshot = decompress(new Uint8Array(event.data));
      for (let byte = 0; byte < snapshot.length; byte++) {
        const base = byte << 3;
        const value = snapshot[byte];
        for (let bit = 0; bit < 8; bit++) {
          checkboxStates[base + bit] = (value & (1 << bit)) !== 0;
        }
      }
      receivedSnapshot = true;
    } else {
      const view = new DataView(event.data);
      for (let i = 0; i < view.byteLength; i += 4) {
        const packed = view.getUint32(i, false);
        checkboxStates[packed & 0x7fffffff] = (packed >>> 31) === 1;
      }
    }

    handleScroll();
  };

  websocket.onclose = () => {
    console.log('WebSocket connection closed');
  };

  container.addEventListener('scroll', handleScroll);
  handleScroll();

  return () => websocket.close();
}
