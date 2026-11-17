import { type HTMLAttributes } from "react";

type Variant = "default" | "secondary" | "outline" | "destructive" | "success";

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: Variant;
}
const variants: Record<Variant, string> = {
  default: "win95-badge-navy",
  secondary: "win95-badge-grey",
  outline: "bg-win95-grey text-black",
  destructive: "win95-badge-red",
  success: "win95-badge-green",
};

export function Badge({ variant = "default", className = "", ...props }: BadgeProps) {
  return (
    <span
      className={`win95-badge ${variants[variant]} ${className}`}
      {...props}
    />
  );
}
