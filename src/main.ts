import './style.css';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { clamp, exportApo, exportPipeWire, flatProfile, uid, validateProfile, type Band, type Ear, type Profile } from './core';
import { EqGraph } from './graph';
import { Audition } from './audition';

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const initial = flatProfile();
let profiles: Profile[] = [initial], activeId = initial.id, ear: Ear = 'left', linked = false, view = 'overlay', selected: string | null = 'band-3', enabled = true;
let saveTimer: ReturnType<typeof setTimeout>;
type Settings = { profileMode: 'perDevice' | 'global'; startAtLogin: boolean; deviceProfiles: Record<string, string>; globalProfileId: string | null; lastOutputId: string | null };
let settings: Settings = { profileMode: 'perDevice', startAtLogin: true, deviceProfiles: {}, globalProfileId: null, lastOutputId: null };
let revision = 0, acknowledged: string | null = null, editGeneration = 0, pendingSave = false;
let latestOutputReport: OutputReport | null = null;
let nativeRate: number | null = null;
const audition = new Audition();
const profile = () => profiles.find(p => p.id === activeId)!;
function notice(message: string, error = false) { $('notice').textContent = message; $('notice').className = error ? 'notice error' : 'notice'; }
function readStore(raw: string) {
  const value = JSON.parse(raw);
  if (!Array.isArray(value.profiles) || !value.profiles.length || value.profiles.length > 100) throw new Error('Invalid saved profiles.');
  const checked = value.profiles.map((value: unknown) => manualProfile(value));
  if (new Set(checked.map((p: Profile) => p.id)).size !== checked.length) throw new Error('Duplicate profile identity.');
  profiles = checked; activeId = profiles.some(p => p.id === value.activeId) ? value.activeId : profiles[0].id;
  enabled = value.enabled !== false; view = value.view === 'focus' ? 'focus' : 'overlay';
  const prefs = value.settings ?? {};
  const bindings = Object.fromEntries(Object.entries(prefs.deviceProfiles ?? {}).filter(([key,id]) => key.length <= 1024 && profiles.some(p => p.id === id)));
  settings = { profileMode: prefs.profileMode === 'global' ? 'global' : 'perDevice', startAtLogin: prefs.startAtLogin !== false,
    deviceProfiles: bindings as Record<string,string>, globalProfileId: profiles.some(p => p.id === prefs.globalProfileId) ? prefs.globalProfileId : null,
    lastOutputId: typeof prefs.lastOutputId === 'string' ? prefs.lastOutputId : null };
  revision = typeof value.revision === 'number' ? value.revision : 0;
}
let storageError = '';
function manualProfile(value: unknown): Profile {
 const checked = validateProfile(value);
 // Manual gain is the agreed product mode, including profiles from the early preview.
 checked.autoHeadroom = false;
 checked.panMode = 'balance';
 return checked;
}
try { const raw = localStorage.getItem('peace2-v1'); if (raw) readStore(raw); }
catch { storageError = 'Saved data could not be read. A flat profile is open; the saved copy has not been overwritten.'; }

$('app').innerHTML = `
 <div class="shell">
  <header class="topbar"><div class="brand"><span class="logo">e<span>q</span></span><div><strong>easy <span class="two">eq</span></strong><small>YOUR SOUND, IN BALANCE</small></div></div>
   <div class="top-status"><span class="status-dot"></span><span id="engine-status">Editor preview</span></div>
   <button id="power" class="power" aria-pressed="true">EQ enabled</button>
  </header>
  <div class="preview-banner" id="audio-status-detail">Checking the native system audio connection…</div>
  <main><aside>
   <div class="section-label">WORKSPACE</div><button class="nav active">◉ &nbsp; Headphone EQ</button>
   <div class="section-label profile-label">PROFILES <button id="new-profile" aria-label="New profile">＋</button></div><div id="profiles"></div>
   <div class="profile-actions"><button id="duplicate">Duplicate</button><button id="rename">Rename</button><button id="delete-profile">Delete</button></div>
   <div class="aside-bottom"><div class="section-label">A SMALLER AUDIO TOOLKIT</div><p>Independent ears.<br>Precise correction.<br>Room to breathe.</p><div class="platforms"><span>WIN</span><span>MAC</span><span>LINUX</span></div><small>Shared editor · native audio adapters in development</small></div>
  </aside><section class="workspace">
   <div class="workspace-heading"><div><div class="eyebrow">HEADPHONE WORKSPACE</div><h1 id="profile-name"></h1><p>A little less harsh. A little more you.</p></div><div class="file-actions"><button id="import">Import profile</button><button id="export-json">Save JSON ↗</button></div></div>
   <div class="eq-panel"><div class="eq-toolbar"><div class="tabs" role="group" aria-label="Edit ear"><button id="left" class="ear left active">Left ear</button><button id="right" class="ear right">Right ear</button><button id="linked">Link edits</button></div><div class="view-controls"><label for="view">View</label><select id="view"><option value="overlay">Both curves</option><option value="focus">One ear at a time</option></select></div></div>
    <div class="graph-legend"><span class="left-dot">LEFT</span><span class="right-dot">RIGHT</span><span id="spectrum-label">SPECTRUM · LOAD AUDIO TO AUDITION</span></div>
    <canvas id="graph" aria-label="Parametric EQ graph. Use the band fields below for keyboard editing."></canvas>
    <div class="graph-footer"><span>Drag to shape · scroll to change Q · double-click to add a band</span><span>MINIMUM PHASE <b>0 added buffer*</b></span></div>
   </div>
   <div class="edit-row"><section class="band-editor"><div class="card-heading"><span class="section-label">SELECTED BAND</span><div><button id="add-band">＋ Band</button><button id="remove-band" aria-label="Delete selected band">−</button></div></div><div class="band-fields">
    <label>Band<select id="band-select"></select></label><label>Shape<select id="shape"><option value="peak">Bell</option><option value="lowShelf">Low shelf</option><option value="highShelf">High shelf</option></select></label>
    <label>Frequency <span>Hz</span><input id="frequency" type="number" min="10" max="20000" step="1" /></label><label>Gain <span>dB</span><input id="band-gain" type="number" min="-24" max="24" step="0.1" /></label><label>Q<input id="q" type="number" min="0.1" max="30" step="0.05" /></label><label class="check-label"><input id="band-enabled" type="checkbox" />On</label>
   </div></section><section class="balance-card"><div class="card-heading"><span class="section-label">STEREO BALANCE</span><button id="center-pan">Center</button></div><div class="balance-readout"><span>L</span><strong id="pan-value">CENTER</strong><span>R</span></div><input id="pan" type="range" min="-1" max="1" step="0.01" value="0" aria-label="Stereo balance" /><small>Preserves channel separation for spatial games.</small></section></div>
   <div class="lower-row"><section class="headroom-card"><div class="card-heading"><span class="section-label">OUTPUT GAIN</span><span class="mini-label">MANUAL</span></div><div class="gain-line"><label>Gain <input id="output-gain" type="number" min="-60" max="0" step="0.5" value="0" /> dB</label><div><small>COMMON PREAMP</small><strong id="total-gain">0.0 dB</strong></div></div><small>The same gain applies to both ears. Reduce it to leave room for EQ boosts.</small></section>
    <section class="audition-card"><div class="card-heading"><span class="section-label">LISTEN & SEE</span><span class="mini-label">LOCAL FILE ONLY</span></div><div class="audition-buttons"><button id="load-audio">Choose audio</button><button id="play-audio" disabled>Play</button><span id="audio-name">No audio loaded</span></div><label class="volume-label">Audition volume <input id="audition-volume" type="range" min="0" max="1" step="0.01" value="0.25" /></label></section></div>
   <section class="outputs-card"><div class="card-heading"><span class="section-label">AUDIO OUTPUTS</span><button id="refresh-outputs">Refresh</button></div><div class="output-settings"><label>Profiles<select id="profile-mode"><option value="perDevice">Remember a profile for each output</option><option value="global">One shared profile for all outputs</option></select></label><label class="check-label"><input id="start-at-login" type="checkbox" checked />Start in tray at sign-in</label><small id="startup-status"></small></div><div id="outputs" aria-live="polite"><p>Detecting outputs…</p></div></section>
   <footer><span id="notice" class="notice" role="status">Changes save automatically.</span><div><select id="export-platform" aria-label="Export audio backend"><option value="apo">Windows · APO</option><option value="pipewire">Linux · PipeWire</option><option value="mac">macOS · profile JSON</option></select><button id="export-config">Export configuration ↗</button></div></footer>
   <p class="footnote">* Native EQ adds no processing buffer. Device latency and spatial paths still need measurement. File audition uses a separate audio engine.</p>
  </section></main>
 </div><input id="profile-file" type="file" accept=".json,application/json" hidden /><input id="audio-file" type="file" accept="audio/*" hidden />
 <dialog id="name-dialog"><form id="name-form"><h2 id="dialog-title">Name profile</h2><label>Profile name<input id="name-input" maxlength="80" required /></label><div><button type="button" id="dialog-cancel">Cancel</button><button type="submit" class="primary">Save</button></div></form></dialog>`;

const graph = new EqGraph($<HTMLCanvasElement>('graph'), {
 profile, rate: () => nativeRate ?? audition.context?.sampleRate ?? 48000, ear: () => ear, view: () => view, selected: () => selected,
 spectrum: () => { const bins = audition.spectrum(); return bins ? { bins, rate: audition.context!.sampleRate } : null; },
 onSelect: (side, id) => { ear = side; selected = id; render(); },
 onDrag: (frequency, gain) => editBand({ frequency, gain }),
 onQ: delta => { const b = selectedBand(); if (b) editBand({ q: Math.round(clamp(b.q * (1 + delta), 0.1, 30) * 100) / 100 }); },
 onAdd: (frequency, gain) => addBand(frequency, gain),
});
const selectedBand = () => profile().ears[ear].find(b => b.id === selected);
function save() {
 clearTimeout(saveTimer);
 pendingSave = true;
 const generation = ++editGeneration;
 saveTimer = setTimeout(async () => {
   const raw = JSON.stringify({ profiles, activeId, enabled, view, settings, revision });
   try {
     if (isTauri()) {
       const saved = await invoke<string>('save_workspace', { json: raw, baseJson: acknowledged });
       if (generation !== editGeneration) return;
       if (JSON.parse(saved).revision >= revision) { readStore(saved); acknowledged = saved; audition.update(profile(), enabled); render(); if (latestOutputReport) renderOutputs(latestOutputReport); }
       localStorage.setItem('peace2-v1', saved);
     } else localStorage.setItem('peace2-v1', raw);
     pendingSave = false; notice('Saved on this device.');
   }
   catch (error) { notice(`Could not save: ${String(error)}`, true); }
 }, 180);
}
function selectProfile(id: string) {
 activeId = id;
 if (settings.profileMode === 'global') settings.globalProfileId = id;
 else if (settings.lastOutputId) settings.deviceProfiles[settings.lastOutputId] = id;
}
function change() { audition.update(profile(), enabled); render(); save(); }
function editBand(patch: Partial<Band>) {
 const b = selectedBand(); if (!b) return;
 Object.assign(b, patch);
 if (linked) {
   const other = profile().ears[ear === 'left' ? 'right' : 'left'].find(x => x.id === b.id);
   if (other) Object.assign(other, patch);
 }
 change();
}
function addBand(frequency = 1000, gain = 0) {
 const p = profile();
 if (p.ears[ear].length >= 31 || (linked && p.ears[ear === 'left' ? 'right' : 'left'].length >= 31)) { notice('This profile supports up to 31 bands per ear.', true); return; }
 const band: Band = { id: uid(), kind: 'peak', frequency, gain, q: 1.4, enabled: true };
 p.ears[ear].push(band); if (linked) p.ears[ear === 'left' ? 'right' : 'left'].push(structuredClone(band)); selected = band.id; change();
}
function render() {
 const p = profile(); $('profile-name').textContent = p.name;
 const list = $('profiles'); list.replaceChildren();
 for (const entry of profiles) {
   const button = document.createElement('button'); button.className = `profile ${entry.id === activeId ? 'active' : ''}`;
   const dot = document.createElement('span'); dot.className = 'profile-dot';
   const name = document.createElement('span'); name.textContent = entry.name;
   button.append(dot, name); button.onclick = () => { selectProfile(entry.id); selected = profile().ears[ear][0]?.id ?? null; change(); }; list.append(button);
 }
 $('left').classList.toggle('active', ear === 'left'); $('right').classList.toggle('active', ear === 'right'); $('linked').classList.toggle('active', linked); $('linked').setAttribute('aria-pressed', String(linked));
 $<HTMLSelectElement>('view').value = view;
 $<HTMLSelectElement>('profile-mode').value = settings.profileMode;
 $<HTMLInputElement>('start-at-login').checked = settings.startAtLogin;
 const bands = p.ears[ear], selector = $<HTMLSelectElement>('band-select'); selector.replaceChildren();
 bands.forEach((b, i) => { const option = new Option(`${i + 1} · ${Math.round(b.frequency)} Hz`, b.id); selector.add(option); });
 if (!bands.some(b => b.id === selected)) selected = bands[0]?.id ?? null;
 selector.value = selected ?? '';
 const b = selectedBand();
 for (const id of ['shape', 'frequency', 'band-gain', 'q', 'band-enabled', 'remove-band']) ($<HTMLInputElement>(id)).disabled = !b;
 if (b) {
   $<HTMLSelectElement>('shape').value = b.kind;
   for (const [id, value] of [['frequency', b.frequency], ['band-gain', b.gain], ['q', b.q]] as const) {
     if (document.activeElement !== $(id)) $<HTMLInputElement>(id).value = String(value);
   }
   $<HTMLInputElement>('band-enabled').checked = b.enabled;
 }
 $<HTMLInputElement>('pan').value = String(p.pan); $('pan-value').textContent = p.pan === 0 ? 'CENTER' : `${Math.round(Math.abs(p.pan) * 100)}% ${p.pan < 0 ? 'LEFT' : 'RIGHT'}`;
 if (document.activeElement !== $('output-gain')) $<HTMLInputElement>('output-gain').value = String(p.gainDb);
 $('total-gain').textContent = `${p.gainDb.toFixed(1)} dB`;
 $('power').textContent = enabled ? 'EQ enabled' : 'EQ bypassed'; $('power').classList.toggle('off', !enabled); $('power').setAttribute('aria-pressed', String(enabled));
 $('spectrum-label').textContent = audition.media.paused ? 'SPECTRUM · LOAD AUDIO TO AUDITION' : 'SPECTRUM · FILE OUTPUT';
 graph.dirty = true;
}
async function download(name: string, content: string, mime = 'text/plain'): Promise<boolean> {
 if (isTauri()) return invoke<boolean>('save_export', { name, content });
 const url = URL.createObjectURL(new Blob([content], { type: mime })), a = document.createElement('a');
 a.href = url; a.download = name; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
 return true;
}
let nameAction: (name: string) => void = () => {};
function nameDialog(title: string, value: string, action: (name: string) => void) {
 nameAction = action; $('dialog-title').textContent = title; $<HTMLInputElement>('name-input').value = value;
 $<HTMLDialogElement>('name-dialog').showModal(); $<HTMLInputElement>('name-input').select();
}
$('name-form').addEventListener('submit', e => { e.preventDefault(); const name = $<HTMLInputElement>('name-input').value.trim(); if (name) { nameAction(name); $<HTMLDialogElement>('name-dialog').close(); } });
$('dialog-cancel').onclick = () => $<HTMLDialogElement>('name-dialog').close();
$('new-profile').onclick = () => nameDialog('New profile', 'My headphones', name => { if (profiles.length >= 100) { notice('Maximum of 100 profiles.', true); return; } const p = flatProfile(name); profiles.push(p); selectProfile(p.id); selected = p.ears[ear][0].id; change(); });
$('duplicate').onclick = () => nameDialog('Duplicate profile', `${profile().name.slice(0, 70)} copy`, name => { if (profiles.length >= 100) { notice('Maximum of 100 profiles.', true); return; } const p = structuredClone(profile()); p.id = uid(); p.name = name; profiles.push(p); selectProfile(p.id); change(); });
$('rename').onclick = () => nameDialog('Rename profile', profile().name, name => { profile().name = name; change(); });
$('delete-profile').onclick = () => { if (profiles.length === 1) { notice('Keep at least one profile.'); return; } const removed = activeId; profiles = profiles.filter(p => p.id !== removed); settings.deviceProfiles = Object.fromEntries(Object.entries(settings.deviceProfiles).filter(([,id]) => id !== removed)); if (settings.globalProfileId === removed) settings.globalProfileId = profiles[0].id; selectProfile(profiles[0].id); selected = null; change(); };
$('left').onclick = () => { ear = 'left'; render(); }; $('right').onclick = () => { ear = 'right'; render(); }; $('linked').onclick = () => { linked = !linked; render(); notice(linked ? 'Linked edits affect bands with matching identities; existing ear differences remain.' : 'Editing ears independently.'); };
$('view').onchange = () => { view = $<HTMLSelectElement>('view').value; render(); save(); };
$('band-select').onchange = () => { selected = $<HTMLSelectElement>('band-select').value; render(); };
$('shape').onchange = () => editBand({ kind: $<HTMLSelectElement>('shape').value as Band['kind'] });
for (const [id, key, min, max] of [['frequency', 'frequency', 10, 20000], ['band-gain', 'gain', -24, 24], ['q', 'q', 0.1, 30]] as const) {
 $(id).addEventListener('change', () => { const field = $<HTMLInputElement>(id), value = field.valueAsNumber; if (Number.isFinite(value)) { const checked = clamp(value, min, max); field.value = String(checked); editBand({ [key]: checked }); } else { field.value = String(selectedBand()?.[key] ?? min); } });
}
$('band-enabled').onchange = () => editBand({ enabled: $<HTMLInputElement>('band-enabled').checked });
$('add-band').onclick = () => addBand();
$('remove-band').onclick = () => { const id = selected; for (const side of linked ? ['left', 'right'] as const : [ear]) profile().ears[side] = profile().ears[side].filter(b => b.id !== id); selected = null; change(); };
$('pan').oninput = () => { profile().pan = $<HTMLInputElement>('pan').valueAsNumber; profile().panMode = 'balance'; change(); };
$('center-pan').onclick = () => { profile().pan = 0; profile().panMode = 'balance'; change(); };
$('output-gain').onchange = () => { const value = $<HTMLInputElement>('output-gain').valueAsNumber; profile().gainDb = Number.isFinite(value) ? clamp(value, -60, 0) : 0; $<HTMLInputElement>('output-gain').value = String(profile().gainDb); change(); };
$('power').onclick = () => { enabled = !enabled; change(); };
function importProfile(raw: string) {
 if (profiles.length >= 100) throw new Error('Maximum of 100 profiles.');
 const p = manualProfile(JSON.parse(raw)); p.id = uid(); profiles.push(p); selectProfile(p.id); selected = null; change(); notice('Profile imported with manual gain.');
}
$('import').onclick = async () => {
 if (!isTauri()) { $('profile-file').click(); return; }
 try { const raw = await invoke<string | null>('import_profile'); if (raw !== null) importProfile(raw); }
 catch (error) { notice(`Could not import: ${String(error)}`, true); }
};
$('profile-file').onchange = async () => {
 const field = $<HTMLInputElement>('profile-file'), file = field.files?.[0]; if (!file) return;
 try { if (file.size > 1024 * 1024) throw new Error('Profile exceeds 1 MB.'); importProfile(await file.text()); }
 catch (error) { notice(String(error), true); } finally { field.value = ''; }
};
$('export-json').onclick = async () => {
 try { if (await download('easy-eq-profile.json', JSON.stringify(profile(), null, 2), 'application/json')) notice('Profile exported.'); }
 catch (error) { notice(`Could not export: ${String(error)}`, true); }
};
$('export-config').onclick = async () => {
 const target = $<HTMLSelectElement>('export-platform').value;
 try {
   const saved = target === 'apo' ? await download('easy-eq.txt', exportApo(profile(), enabled))
     : target === 'pipewire' ? await download('easy-eq-filter-chain.conf', exportPipeWire(profile(), enabled))
     : await download('easy-eq-macos-profile.json', JSON.stringify(profile(), null, 2), 'application/json');
   if (saved) notice(target === 'mac' ? 'Shared profile exported. The Core Audio driver is still in development.' : 'Configuration exported. Device activation is a separate setup step.');
 } catch (error) { notice(`Could not export: ${String(error)}`, true); }
};
$('load-audio').onclick = () => $('audio-file').click();
$('audio-file').onchange = async () => {
 const field = $<HTMLInputElement>('audio-file'), file = field.files?.[0]; if (!file) return;
 try { await audition.open(file, profile(), enabled); $('audio-name').textContent = file.name; $<HTMLButtonElement>('play-audio').disabled = false; $('play-audio').textContent = 'Play'; render(); notice('Audio loaded locally. Press Play to audition this EQ.'); }
 catch (error) { notice(`Audio could not load: ${String(error)}`, true); } finally { field.value = ''; }
};
$('play-audio').onclick = async () => { try { await audition.toggle(); $('play-audio').textContent = audition.media.paused ? 'Play' : 'Pause'; render(); } catch (error) { notice(String(error), true); } };
$('audition-volume').oninput = () => audition.setVolume($<HTMLInputElement>('audition-volume').valueAsNumber);
audition.media.onerror = () => { notice('This audio file could not be decoded. Try WAV or MP3.', true); $<HTMLButtonElement>('play-audio').disabled = true; };
window.addEventListener('beforeunload', () => { audition.media.pause(); });
render(); if (storageError) notice(storageError, true);
type OutputReport = { outputs: { id: string; name: string; state: string; isDefault: boolean; support: string; reason: string }[]; diagnostics: string[] };
function renderOutputs(report: OutputReport) {
 latestOutputReport = report;
 const container = $('outputs'); container.replaceChildren();
 const message = (text: string) => { const p = document.createElement('p'); p.textContent = text; container.append(p); };
 if (!report.outputs.length) message('No audio outputs were returned by the operating system. Connect an output and refresh.');
 for (const output of report.outputs) {
   const row = document.createElement('div'); row.className = 'output-row';
   const heading = document.createElement('div'); heading.className = 'output-heading';
   const name = document.createElement('strong'); name.textContent = output.name;
   const state = document.createElement('span'); state.className = `output-state ${output.support}`;
   state.textContent = ({processing:'Processing',partial:'Front L/R only',bypassed:'Bypassed',waiting:'Waiting for playback',connecting:'Connecting',needsSetup:'Setup needed',error:'Error'} as Record<string,string>)[output.support] ?? 'Unavailable';
   heading.append(name, state);
   if (output.isDefault) { const badge = document.createElement('span'); badge.className = 'output-default'; badge.textContent = 'System default'; heading.append(badge); }
   const reason = document.createElement('p'); reason.textContent = output.reason;
   const binding = settings.deviceProfiles[output.id];
   const assigned = profiles.find(p => p.id === (settings.profileMode === 'global' ? settings.globalProfileId : binding));
   if (assigned) reason.textContent += ` Profile: ${assigned.name}.`;
   row.append(heading, reason);
   if (settings.profileMode === 'perDevice') {
     const chooser = document.createElement('select'); chooser.setAttribute('aria-label', `Profile for ${output.name}`);
     if (!binding) { const option = new Option('Choose a profile for this output', ''); option.disabled = true; chooser.add(option); }
     profiles.forEach(p => chooser.add(new Option(p.name, p.id))); chooser.value = binding ?? '';
     chooser.onchange = () => {
       settings.deviceProfiles[output.id] = chooser.value;
       if (output.isDefault) { activeId = chooser.value; selected = null; change(); }
       else save();
     };
     row.append(chooser);
   }
   container.append(row);
 }
 report.diagnostics.forEach(message);
}
async function refreshOutputs() {
 const container = $('outputs'), button = $<HTMLButtonElement>('refresh-outputs');
 button.disabled = true; container.replaceChildren();
 const message = (text: string) => { const p = document.createElement('p'); p.textContent = text; container.append(p); };
 try {
   if (!isTauri()) { message('Open the desktop app to detect all system outputs and inspect their support status.'); return; }
   const report = await invoke<OutputReport>('detect_outputs');
   renderOutputs(report);
 } catch (error) { message(`Device discovery failed: ${String(error)}`); }
 finally { button.disabled = false; }
}
$('refresh-outputs').onclick = refreshOutputs;
void refreshOutputs();
async function configureStartup() {
 const status = $('startup-status');
 if (!isTauri()) { status.textContent = 'Sign-in startup is available in the desktop app.'; return; }
 try { const actual = await invoke<boolean>('configure_startup', { enabled: settings.startAtLogin }); status.textContent = actual ? 'Starts in tray and restores the last on/off state.' : 'Sign-in startup is off.'; }
 catch (error) { status.textContent = `Sign-in startup could not be updated: ${String(error)}`; }
}
$('profile-mode').onchange = () => {
 settings.profileMode = $<HTMLSelectElement>('profile-mode').value as Settings['profileMode'];
 if (settings.profileMode === 'global') { settings.globalProfileId ??= activeId; activeId = settings.globalProfileId; }
 else if (settings.lastOutputId && settings.deviceProfiles[settings.lastOutputId]) activeId = settings.deviceProfiles[settings.lastOutputId];
 change(); void refreshOutputs();
};
$('start-at-login').onchange = () => { settings.startAtLogin = $<HTMLInputElement>('start-at-login').checked; save(); void configureStartup(); };
type AudioStatus = {outputId:string;state:string;reason:string;sampleRate:number|null};
function renderAudioStatus(statuses: AudioStatus[]) {
 const current=statuses.find(s => s.outputId===settings.lastOutputId);
 if (current) {
   nativeRate=current.sampleRate;
   $('engine-status').textContent=current.state==='processing' ? 'System EQ processing' : current.state==='partial' ? 'System EQ · front L/R only' : current.state==='bypassed' ? 'System EQ bypassed' : 'System EQ · '+current.state;
   $('audio-status-detail').textContent=current.reason;graph.dirty=true;
 } else { $('engine-status').textContent='System EQ · adapter unavailable';$('audio-status-detail').textContent='This platform’s native system audio adapter is not connected yet. File audition remains available.'; }
}
if (isTauri()) {
 try {
   await listen<string>('workspace-updated', event => {
     if (pendingSave || JSON.parse(event.payload).revision < revision) return;
     readStore(event.payload); acknowledged = event.payload; localStorage.setItem('peace2-v1', event.payload); audition.update(profile(), enabled); render(); void refreshOutputs();
   });
   await listen<OutputReport>('outputs-updated', event => renderOutputs(event.payload));
   await listen<AudioStatus[]>('audio-updated', event => renderAudioStatus(event.payload));
   await listen<string>('outputs-error', event => { $('outputs').textContent = event.payload; });
   const raw = await invoke<string | null>('load_workspace'); if (raw) { readStore(raw); acknowledged = raw; render(); } else save();
   await listen<boolean>('eq-intent', event => { enabled = event.payload; audition.update(profile(), enabled); render(); save(); });
   renderAudioStatus(await invoke<AudioStatus[]>('audio_status'));
   await invoke('editor_ready');
   await configureStartup();
 } catch (error) { notice(`Native storage unavailable: ${String(error)}`, true); }
}
