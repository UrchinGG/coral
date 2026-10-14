import { BrowserRouter, Route, Routes } from "react-router-dom";
import { AppShell } from "./layout/AppShell";
import { Data } from "./pages/Data";
import { Diagnostics } from "./pages/Diagnostics";
import { MemberDetail } from "./pages/MemberDetail";
import { Members } from "./pages/Members";
import { Overview } from "./pages/Overview";
import { PlayerDetail } from "./pages/PlayerDetail";
import { Players } from "./pages/Players";
import { PluginDetail } from "./pages/PluginDetail";
import { Plugins } from "./pages/Plugins";
import { ServerSync } from "./pages/ServerSync";
import { Starfish } from "./pages/Starfish";
import { StarfishUserDetail } from "./pages/StarfishUserDetail";

function App() {
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
          <Route path="/servers" element={<ServerSync />} />
          <Route path="/starfish" element={<Starfish />} />
          <Route path="/starfish/users/:id" element={<StarfishUserDetail />} />
          <Route path="/starfish/plugins" element={<Plugins />} />
          <Route path="/starfish/plugins/:slug" element={<PluginDetail />} />
          <Route path="/data" element={<Data />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

export default App;
