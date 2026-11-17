import { type ReactNode, type HTMLAttributes } from "react";

type DivProps = HTMLAttributes<HTMLDivElement>;

export function Card({ className = "", ...props }: DivProps) {
  return (
    <div
      className={`win95-panel bg-card text-card-foreground ${className}`}
      {...props}
    />
  );
}

export function CardHeader({ className = "", ...props }: DivProps) {
  return <div className={`flex flex-col space-y-1.5 p-4 ${className}`} {...props} />;
}

export function CardTitle({ className = "", ...props }: DivProps) {
  return <div className={`text-sm font-bold uppercase leading-none text-foreground ${className}`} {...props} />;
}

export function CardContent({ className = "", ...props }: DivProps) {
  return <div className={`p-4 pt-0 ${className}`} {...props} />;
}

export function CardDescription({ className = "", ...props }: DivProps) {
  return <div className={`text-xs text-muted-foreground ${className}`} {...props} />;
}
