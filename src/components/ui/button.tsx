import { type ButtonHTMLAttributes } from "react";

type Variant = "default" | "secondary" | "outline" | "ghost" | "destructive";
type Size = "sm" | "md" | "lg";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
}
const variants: Record<Variant, string> = {
  default: "bg-primary text-primary-foreground hover:opacity-90",
  secondary: "bg-secondary text-secondary-foreground hover:opacity-80",
  outline: "border border-border bg-transparent hover:bg-accent text-foreground",
  ghost: "hover:bg-accent text-foreground",
  destructive: "bg-destructive text-destructive-foreground hover:opacity-90",
};

const sizes: Record<Size, string> = {
  sm: "h-8 px-3 text-xs",
  md: "h-9 px-4 text-sm",
  lg: "h-10 px-6 text-sm",
};

export function Button({ variant = "default", size = "md", className = "", ...props }: ButtonProps) {
  return (
    <button
      className={`inline-flex items-center justify-center rounded-md font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 disabled:pointer-events-none ${variants[variant]} ${sizes[size]} ${className}`}
      {...props}
    />
  );
}
