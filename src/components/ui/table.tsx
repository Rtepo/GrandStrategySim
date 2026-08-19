import { type ThHTMLAttributes, type TdHTMLAttributes, type TableHTMLAttributes, type HTMLAttributes } from "react";

export function Table({ className = "", ...props }: TableHTMLAttributes<HTMLTableElement>) {
  return (
    <div className="w-full overflow-auto">
      <table className={`w-full caption-bottom text-sm ${className}`} {...props} />
    </div>
  );
}

export function TableHeader({ className = "", ...props }: HTMLAttributes<HTMLTableSectionElement>) {
  return <thead className={`[&_tr]:border-b border-border ${className}`} {...props} />;
}

export function TableBody({ className = "", ...props }: HTMLAttributes<HTMLTableSectionElement>) {
  return <tbody className={`[&_tr:last-child]:border-0 ${className}`} {...props} />;
}

export function TableRow({ className = "", ...props }: HTMLAttributes<HTMLTableRowElement>) {
  return <tr className={`border-b border-border transition-colors hover:bg-accent/50 ${className}`} {...props} />;
}

export function TableHead({ className = "", ...props }: ThHTMLAttributes<HTMLTableCellElement>) {
  return <th className={`h-9 px-3 text-left align-middle font-medium text-muted-foreground text-xs uppercase tracking-wider ${className}`} {...props} />;
}

export function TableCell({ className = "", ...props }: TdHTMLAttributes<HTMLTableCellElement>) {
  return <td className={`px-3 py-2 align-middle ${className}`} {...props} />;
}

export function TableEmpty({ colSpan, message }: { colSpan: number; message: string }) {
  return (
    <TableRow>
      <TableCell colSpan={colSpan} className="text-center text-muted-foreground py-8">
        {message}
      </TableCell>
    </TableRow>
  );
}
