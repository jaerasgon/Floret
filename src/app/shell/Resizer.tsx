import { useCallback, useRef, useState } from 'react';

export function Resizer({ onResize }: { onResize: (dx: number) => void }) {
  const [dragging, setDragging] = useState(false);
  const lastX = useRef(0);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      setDragging(true);
      lastX.current = e.clientX;
    },
    [],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!dragging) return;
      const dx = e.clientX - lastX.current;
      lastX.current = e.clientX;
      onResize(dx);
    },
    [dragging, onResize],
  );

  const handlePointerUp = useCallback((e: React.PointerEvent) => {
    (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    setDragging(false);
  }, []);

  return (
    <div
      className={`resizer${dragging ? ' dragging' : ''}`}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
    />
  );
}
