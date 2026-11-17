import { type ReactNode, useState } from "react";

interface TabsProps {
  tabs: { label: string; value: string; content: ReactNode }[];
  defaultValue?: string;
}

export function Tabs({ tabs, defaultValue }: TabsProps) {
  const [active, setActive] = useState(defaultValue ?? tabs[0]?.value);

  return (
    <div>
      <div className="flex flex-wrap mb-0">
        {tabs.map((tab) => (
          <button
            key={tab.value}
            onClick={() => setActive(tab.value)}
            className={`win95-tab text-xs font-bold uppercase whitespace-nowrap ${
              active === tab.value ? "win95-tab-active font-bold" : ""
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div className="win95-panel p-2">
        {tabs.find((t) => t.value === active)?.content}
      </div>
    </div>
  );
}
