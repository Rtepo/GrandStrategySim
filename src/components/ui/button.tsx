import { type ButtonHTMLAttributes } from "react";

type Variant = "default" | "secondary" | "outline" | "ghost" | "destructive";
type Size = "sm" | "md" | "lg";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
}
const variants: Record<Variant, string> = {
  default: "bg-win95-grey text-black",
  secondary: "bg-win95-light text-black",
  outline: "bg-win95-grey text-black",
  ghost: "bg-win95-grey text-black",
  destructive: "bg-win95-grey text-win95-red font-bold",
};

const sizes: Record<Size, string> = {
  sm: "px-2 py-0.5 text-xs",
  md: "px-3 py-1 text-sm",
  lg: "px-4 py-1.5 text-sm",
};

export function Button({ variant = "default", size = "md", className = "", ...props }: ButtonProps) {
  return (
    <button
      className={`inline-flex items-center justify-center font-medium disabled:text-win95-dark disabled:pointer-events-none ${variants[variant]} ${sizes[size]} ${className}`}
      {...props}
    />
  );
}
