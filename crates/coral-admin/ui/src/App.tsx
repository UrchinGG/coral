import { BrowserRouter, Route, Routes } from "react-router-dom";
import { useMe } from "./api/auth";
import { AppShell } from "./layout/AppShell";
import { Login } from "./layout/Login";
import { Data } from "./pages/Data";
import { Diagnostics } from "./pages/Diagnostics";
import { MemberDetail } from "./pages/MemberDetail";
import { Members } from "./pages/Members";
import { Overview } from "./pages/Overview";
import { PlayerDetail } from "./pages/PlayerDetail";
import { Players } from "./pages/Players";
import { PluginDetail } from "./pages/PluginDetail";
import { Plugins } from "./pages/Plugins";

function App() {
  const me = useMe();

  if (me.isLoading) {
    return <div className="flex min-h-screen items-center justify-center bg-[#0b0e14] text-gray-500">Loading…</div>;
  }

  if (me.isError || !me.data?.authenticated) {
    return <Login />;
  }

  return (
    <BrowserRouter>
      <Routes>
        <Route element={<AppShell />}>
          <Route path="/" element={<Overview />} />
          <Route path="/diagnostics" element={<Diagnostics />} />
          <Route path="/members" element={<Members />} />
          <Route path="/members/:id" element={<MemberDetail />} />
          <Route path="/players" element={<Players />} />
          <Route path="/players/:uuid" element={<PlayerDetail />} />
          <Route path="/plugins" element={<Plugins />} />
          <Route path="/plugins/:slug" element={<PluginDetail />} />
          <Route path="/data" element={<Data />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

export default App;
