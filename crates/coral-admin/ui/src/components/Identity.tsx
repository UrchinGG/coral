import { Link } from "react-router-dom";

type IdentityProps = {
  id: string | number | null | undefined;
  username?: string | null;
  kind?: "discord" | "minecraft";
  linkTo?: string;
};

export function Identity({ id, username, kind = "discord", linkTo }: IdentityProps) {
  if (id === null || id === undefined || id === "") {
    return <span className="text-gray-500">—</span>;
  }
  const label = username ? (kind === "discord" ? `@${username}` : username) : String(id);
  const content = (
    <span className="inline-flex flex-col leading-tight" title={String(id)}>
      <span className={username ? "text-gray-100" : "font-mono text-xs text-gray-400"}>{label}</span>
      {username && <span className="font-mono text-[10px] text-gray-500">{String(id)}</span>}
    </span>
  );
  return linkTo ? (
    <Link to={linkTo} className="hover:underline" onClick={(e) => e.stopPropagation()}>
      {content}
    </Link>
  ) : (
    content
  );
}
