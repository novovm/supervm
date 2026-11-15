import { mkdir, open, rm } from 'node:fs/promises';
import path from 'node:path';
import { Broker, type Engines, type Peer } from './broker';
import { id, VERSION, TOOL_CAPABILITY, NodeId, EngineControls } from './protocol';
import { startQuotaAdmin } from './quota-admin';

export class Application {
  private constructor(readonly broker: Broker, readonly peers: Peer[], readonly engines: Engines,
    private release: () => Promise<void>) {}
  static async open(directory: string, peers: Peer[], engines: Engines): Promise<Application> {
    if (!path.isAbsolute(directory)) throw Error('Absolute private ASAS directory required');
    if (!peers.length || new Set(peers.map(peer => peer.id)).size !== peers.length || peers.some(peer =>
      !peer.id || !peer.projects.length || peer.projects.some(project => !/^[a-f0-9]{64}$/.test(project)))) throw Error('Invalid peer grants');
    if (!Object.keys(engines).length || Object.entries(engines).some(([name, engine]) => !['codex', 'claude'].includes(name)
      || !engine?.models.length || engine.models.some(model => !model.trim() || model.length > 200))) throw Error('Explicit engine/model allowlists required');
    await mkdir(directory, { recursive: true, mode: 0o700 });
    const lockPath = path.join(directory, 'owner.lock');
    const lock = await open(lockPath, 'wx', 0o600).catch(() => { throw Error('ASAS directory locked; inspect the owning process before recovery'); });
    let released = false;
    const release = async () => { if (!released) { released = true; await lock.close(); await rm(lockPath, { force: true }); } };
    const broker = new Broker(directory, engines);
    try { await lock.writeFile(JSON.stringify({ pid: process.pid })); await lock.sync(); await broker.load(); }
    catch (error) { await release(); throw error; }
    try {
      const admin = await startQuotaAdmin(directory, broker.meter, peers.map(peer => peer.id));
      return new Application(broker, peers, engines, async () => { try { await admin.close(); } finally { await release(); } });
    } catch (error) { await broker.close(); await release(); throw error; }
  }
  health() { return { version: VERSION, toolContract: TOOL_CAPABILITY, engines: Object.fromEntries(Object.entries(this.engines).map(([name, value]) =>
    [name, { models: value!.models, inferenceVerified: false }])), remoteRouteReady: false }; }
  // Only an authenticated transport owner may resolve this local principal.
  async dispatch(peerId: string, operation: string, input: any, nodeId?: string): Promise<unknown> {
    const grant = this.peers.find(peer => peer.id === peerId);
    if (!grant) throw Error('403 unknown application principal');
    const peer = { ...grant, nodeId: nodeId === undefined ? undefined : NodeId.parse(nodeId) };
    const ownUsage = () => this.engines.claude ? { claude: this.broker.meter.view(peer.id, peer.nodeId) } : {};
    if (operation === 'capabilities') {
      if (this.engines.claude) await this.broker.meter.touch(peer.id, peer.nodeId);
      const health = this.health();
      for (const [name, engine] of Object.entries(this.engines)) {
        if (!engine?.controls) continue;
        try { Object.assign(health.engines[name]!, { controls: EngineControls.parse(await engine.controls()) }); }
        catch { /* Unknown controls must not prevent ordinary shared programming. */ }
      }
      return { ...health, peerId: peer.id, projects: peer.projects, ownUsage: ownUsage(), ...(peer.nodeId ? { workerNodeId: peer.nodeId } : {}) };
    }
    if (operation === 'submit') { const task = await this.broker.submit(peer, input); return { id: task.id, status: task.status }; }
    const taskId = id.parse(input?.taskId);
    if (operation === 'events') return { ...await this.broker.events(peer, taskId, input.after, input.waitMs), ownUsage: ownUsage() };
    if (operation === 'result') { await this.broker.result(peer, taskId, input.result); return { accepted: true }; }
    if (operation === 'cancel') { await this.broker.cancel(peer, taskId); return { cancelRequested: true }; }
    throw Error('400 unknown application operation');
  }
  async close() { try { await this.broker.close(); } finally { await this.release(); } }
}
