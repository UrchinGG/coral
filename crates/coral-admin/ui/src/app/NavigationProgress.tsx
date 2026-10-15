import { useNavigation } from "react-router-dom";

export function NavigationProgress() {
  const navigation = useNavigation();
  if (navigation.state === "idle") return null;
  return (
    <div aria-hidden className="pointer-events-none fixed inset-x-0 top-[env(safe-area-inset-top,0px)] z-[100] h-0.5 overflow-hidden">
      <div className="h-full w-2/5 animate-indeterminate bg-accent" />
    </div>
  );
}
