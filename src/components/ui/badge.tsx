import { type HTMLAttributes } from "react";

type Variant = "default" | "secondary" | "outline" | "destructive" | "success";

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: Variant;
}
const variants: Record<Variant, string> = {
  default: "bg-primary/20 text-primary border-primary/30",
  secondary: "bg-secondary text-secondary-foreground border-border",
  outline: "border-border text-foreground",
  destructive: "bg-destructive/20 text-destructive border-destructive/30",
  success: "bg-green-500/20 text-green-400 border-green-500/30",
};

export function Badge({ variant = "default", className = "", ...props }: BadgeProps) {
  return (
    <span
      className={`inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium ${variants[variant]} ${className}`}
      {...props}
    />
  );
}
