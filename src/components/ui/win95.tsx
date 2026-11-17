import { type ReactNode, type HTMLAttributes } from "react";

type DivProps = HTMLAttributes<HTMLDivElement>;

interface Win95WindowProps extends Omit<DivProps, "title"> {
  /** Text shown in the deep-blue titlebar. */
  title?: ReactNode;
  /** Optional right-side content next to the mock close button. */
  titleExtra?: ReactNode;
  /** When true, renders the decorative inset [X] window control. */
  closable?: boolean;
  /** Callback for the mock close button (e.g. dismiss an overlay). */
  onClose?: () => void;
  /** Extra classes on the outer beveled frame. */
  frameClassName?: string;
  /** Extra classes on the inner content region. */
  bodyClassName?: string;
}

/**
 * TASK-039: Primary content container styled like an active Windows 95
 * window — thick outset grey frame + navy titlebar + mock [X] control.
 */
export function Win95Window({
  title,
  titleExtra,
  closable = true,
  onClose,
  frameClassName = "",
  bodyClassName = "",
  children,
  ...props
}: Win95WindowProps) {
  return (
    <div className={`win95-window ${frameClassName}`} {...props}>
      {title !== undefined && (
        <div className="win95-titlebar">
          <span>{title}</span>
          <span className="flex items-center gap-2">
            {titleExtra}
            {closable && (
              <span
                className="win95-close-btn"
                role={onClose ? "button" : undefined}
                onClick={onClose}
                title={onClose ? "Close" : undefined}
              >
                X
              </span>
            )}
          </span>
        </div>
      )}
      <div className={`p-2 ${bodyClassName}`}>{children}</div>
    </div>
  );
}

/** Deep-blue section titlebar (e.g. "VOTED FOR", "MARKET TERMINAL"). */
export function Win95Titlebar({ className = "", children, ...props }: DivProps) {
  return (
    <div className={`win95-titlebar ${className}`} {...props}>
      {children}
    </div>
  );
}

/** Raised grey panel — white TL / dark BR bevel. */
export function Win95Panel({ className = "", children, ...props }: DivProps) {
  return (
    <div className={`win95-panel ${className}`} {...props}>
      {children}
    </div>
  );
}

/** Sunken white frame — inset tables, lists, hemicycle wells. */
export function Win95Inset({ className = "", children, ...props }: DivProps) {
  return (
    <div className={`win95-inset ${className}`} {...props}>
      {children}
    </div>
  );
}
