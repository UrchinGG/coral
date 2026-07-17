export function Login() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-[#0b0e14]">
      <div className="flex flex-col items-center gap-4 rounded-lg border border-white/10 bg-white/5 p-10 text-center">
        <h1 className="text-xl font-semibold text-gray-100">Coral Admin</h1>
        <p className="max-w-xs text-sm text-gray-500">
          Sign in with the Discord account registered as an owner to continue.
        </p>
        <a
          href="/auth/login"
          className="rounded bg-indigo-500 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-400"
        >
          Sign in with Discord
        </a>
      </div>
    </div>
  );
}
