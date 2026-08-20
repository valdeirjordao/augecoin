import React from 'react';

export function Skeleton({ rows = 3 }: { rows?: number }) {
  return (
    <div className="skeleton-list" data-testid="skeleton">
      {Array.from({ length: rows }).map((_, i) => (
        <div className="skeleton-row" key={i}>
          <div className="skeleton-cell skeleton-cell-lg" />
          <div className="skeleton-cell" />
        </div>
      ))}
    </div>
  );
}

export function Modal({
  open,
  title,
  onClose,
  children,
}: {
  open: boolean;
  title: string;
  onClose: () => void;
  children: React.ReactNode;
}) {
  if (!open) return null;
  return (
    <div className="modal-backdrop" onClick={onClose} data-testid="modal-backdrop">
      <div className="modal" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h3>{title}</h3>
          <button className="btn btn-ghost" onClick={onClose} aria-label="Fechar">
            ×
          </button>
        </div>
        <div className="modal-body">{children}</div>
      </div>
    </div>
  );
}
