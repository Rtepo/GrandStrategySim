import { type InputHTMLAttributes } from "react";

export function Input({ className = "", ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={`flex h-8 w-full win95-inset px-2 py-1 text-sm text-black placeholder:text-win95-dark focus-visible:outline-dotted ${className}`}
      {...props}
    />
  );
}
