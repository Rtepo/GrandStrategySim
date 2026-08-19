import { type ReactNode, useState } from "react";

interface TabsProps {
  tabs: { label: string; value: string; content: ReactNode }[];
  defaultValue?: string;
}

export function Tabs({ tabs, defaultValue }: TabsProps) {
  const [active, setActive] = useState(defaultValue ?? tabs[0]?.value);

  return (
    <div>
      <div className="flex border-b border-border mb-4">
        {tabs.map((tab) => (
          <button
            key={tab.value}
            onClick={() => setActive(tab.value)}
            className={`px-4 py-2 text-sm font-medium transition-colors border-b-2 -mb-px ${
              active === tab.value
                ? "border-primary text-primary"
                : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {tabs.find((t) => t.value === active)?.content}
    </div>
  );
}
