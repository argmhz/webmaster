// Fælles drag-and-drop: hvad der trækkes, og en indsætningslinje.

export type Drag =
  | { kind: "el"; id: string }
  | { kind: "newEl"; type: string }
  | { kind: "sec"; id: string }
  | { kind: "newSec"; layout: string }
  | { kind: "node"; id: string };

let current: Drag | null = null;
export const getDrag = () => current;

let line: HTMLDivElement | undefined;
function indicator() {
  if (!line) {
    line = document.createElement("div");
    line.className = "drop-line";
    document.body.appendChild(line);
  }
  return line;
}

export function startDrag(e: DragEvent, d: Drag, fade?: HTMLElement, image?: HTMLElement) {
  current = d;
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = "copyMove";
    e.dataTransfer.setData("text/plain", d.kind);
    if (image) {
      try {
        e.dataTransfer.setDragImage(image, 24, 14);
      } catch {
        /* ikke understøttet */
      }
    }
  }
  const el = fade ?? (e.currentTarget as HTMLElement | null);
  if (el) requestAnimationFrame(() => el.classList.add("dragging"));
}

export function clearMarks() {
  if (line) line.style.display = "none";
  document.querySelectorAll(".over, .drop-inside").forEach((x) => x.classList.remove("over", "drop-inside"));
}

export function endDrag() {
  current = null;
  clearMarks();
  document.querySelectorAll(".dragging").forEach((x) => x.classList.remove("dragging"));
}
document.addEventListener("dragend", endDrag);

export function showLine(left: number, width: number, y: number) {
  const l = indicator();
  l.style.display = "block";
  l.style.left = `${left}px`;
  l.style.width = `${width}px`;
  l.style.top = `${y - 1}px`;
}

/** Find indsætningsplads ud fra musens y blandt søskende. */
export function slot(items: Element[], y: number, box: DOMRect) {
  let idx = items.length;
  for (let i = 0; i < items.length; i++) {
    const r = items[i].getBoundingClientRect();
    if (y < r.top + r.height / 2) {
      idx = i;
      break;
    }
  }
  let ly: number;
  if (!items.length) ly = box.top + box.height / 2;
  else if (idx < items.length) ly = items[idx].getBoundingClientRect().top - 4;
  else ly = items[items.length - 1].getBoundingClientRect().bottom + 4;
  return { idx, y: ly };
}
