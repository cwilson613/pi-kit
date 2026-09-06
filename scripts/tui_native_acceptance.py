#!/usr/bin/env python3
"""Drive native macOS terminal trials and capture attributable GUI evidence."""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import time
import uuid

from tui_operator_test import verify_bundle, digest, cleanup_tree


def visible_tail(text, rows):
    return '\n'.join(text.splitlines()[-rows:])


def trial_outcome(manifest):
    return (manifest.get('tui_started') is True and manifest.get('provider_requests') == 4
            and manifest.get('fixture_write_exists') is False and manifest.get('recorder_exit_code') == 0)


def command(args, **kwargs):
    return subprocess.check_output([str(a) for a in args], text=True, timeout=15, **kwargs).strip('\n')


def apple(body, *args):
    return command(['/usr/bin/osascript', '-e', 'on run argv\n'+body+'\nend run', *args])


def process_exists(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


class NativeClient:
    def __init__(self, client, bundle, helper, output):
        self.client, self.bundle, self.helper, self.output = client, bundle, helper, output
        self.token = 'og-native-' + uuid.uuid4().hex[:10]
        self.rows = 40
        self.id = None
        self.session_id = None
        self.window = None
        self.gui = None
        self.notes = []
        self.wez = '/Applications/WezTerm.app/Contents/MacOS/wezterm'
        self.kitty = '/Applications/kitty.app/Contents/MacOS/kitty'

    def windows(self, all_spaces=False):
        return json.loads(command([self.helper, 'windows', *(['--all'] if all_spaces else [])]))

    def close(self):
        """Close the exact trial surface without activating an application."""
        if self.id is None:
            if self.gui is not None and self.gui.poll() is None:
                cleanup_tree(self.gui.pid)
            return
        if self.window is None:
            if self.gui is not None and self.gui.poll() is None:
                cleanup_tree(self.gui.pid)
                return
            raise RuntimeError('native window ownership was not established; refusing window closure')
        if self.window is not None:
            key = (self.window['kCGWindowNumber'], self.window['kCGWindowOwnerPID'])
            if not process_exists(key[1]) or not any((w['kCGWindowNumber'], w['kCGWindowOwnerPID']) == key for w in self.windows(True)):
                return
        if self.client == 'ghostty':
            apple('tell application id "com.mitchellh.ghostty" to close terminal id (item 1 of argv)', self.id)
        elif self.client == 'iterm':
            if self.session_id is None:
                raise RuntimeError('native session ownership was not established')
            apple('''tell application id "com.googlecode.iterm2"
set w to window id (item 1 of argv as integer)
if (count of tabs of w) is not 1 then error "trial window contains other tabs; refusing closure"
if (count of sessions of current tab of w) is not 1 then error "trial tab contains other sessions; refusing closure"
if (unique ID of current session of w) is not (item 2 of argv) then error "trial session changed; refusing closure"
close w
end tell''', self.id, self.session_id)
        elif self.client == 'terminal':
            if self.session_id is None:
                raise RuntimeError('native session ownership was not established')
            apple('''tell application id "com.apple.Terminal"
set w to window id (item 1 of argv as integer)
if (count of tabs of w) is not 1 then error "trial window contains other tabs; refusing closure"
if (tty of selected tab of w) is not (item 2 of argv) then error "trial session changed; refusing closure"
close w saving no
end tell''', self.id, self.session_id)
        elif self.client == 'kitty':
            self.remote('close-window', '--match', 'id:'+self.id)
        else:
            self.remote('kill-pane', '--pane-id', self.id)
            if getattr(self, 'split_id', None) is not None:
                self.remote('kill-pane', '--pane-id', self.split_id)
        if self.window is not None:
            deadline = time.monotonic()+5
            while process_exists(key[1]) and any((w['kCGWindowNumber'], w['kCGWindowOwnerPID']) == key for w in self.windows(True)):
                if time.monotonic() >= deadline:
                    raise RuntimeError('owned native window survived cleanup')
                time.sleep(.1)

    def launch(self):
        before = {w['kCGWindowNumber'] for w in self.windows(True)}
        run = self.bundle / self.client / 'Run.command'
        if self.client == 'ghostty':
            self.id = apple('''tell application id "com.mitchellh.ghostty"
set cfg to new surface configuration
set command of cfg to item 1 of argv
set w to new window with configuration cfg
return id of focused terminal of selected tab of w
end tell''', shlex.join([str(run)]))
        elif self.client == 'iterm':
            identity = apple('''tell application id "com.googlecode.iterm2"
set w to create window with default profile command (item 1 of argv)
return (id of w as text) & linefeed & (unique ID of current session of w)
end tell''', shlex.join([str(run)]))
            self.id, self.session_id = identity.splitlines()
        elif self.client == 'terminal':
            identity = apple('''tell application id "com.apple.Terminal"
set t to do script (item 1 of argv)
set trialTTY to tty of t
repeat with w in windows
repeat with candidate in tabs of w
if tty of candidate is trialTTY then return (id of w as text) & linefeed & trialTTY
end repeat
end repeat
error "cannot find trial terminal"
end tell''', shlex.join([str(run)]))
            self.id, self.session_id = identity.splitlines()
            self.notes.append('Terminal do script appends Return; navigation uses combined sequences. Physical key mapping and native paste are not covered.')
        else:
            if self.client == 'kitty':
                self.socket = '/tmp/' + self.token + '.sock'
                args = [self.kitty, '--listen-on', 'unix:'+self.socket, '-o', 'allow_remote_control=socket-only', '--title', self.token, str(run)]
                self.notes.append('Remote control enabled only on this owned instance through a local Unix socket.')
            else:
                args = [self.wez, 'start', '--always-new-process', '--no-auto-connect', '--class', self.token, '--', str(run)]
            with (self.output/'launcher.log').open('ab') as log:
                self.gui = subprocess.Popen(args, start_new_session=True, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
            deadline = time.monotonic()+20
            while True:
                try:
                    if self.client == 'kitty':
                        self.id = str(json.loads(self.remote('ls'))[0]['tabs'][0]['windows'][0]['id'])
                    else:
                        self.id = str(json.loads(self.remote('list', '--format', 'json'))[0]['pane_id'])
                    break
                except (subprocess.CalledProcessError, IndexError, json.JSONDecodeError):
                    if time.monotonic() > deadline: raise
                    time.sleep(.2)
        deadline = time.monotonic()+15
        while True:
            candidates = [w for w in self.windows() if w['kCGWindowNumber'] not in before]
            owner = {'iterm':'iTerm2', 'terminal':'Terminal', 'ghostty':'Ghostty', 'kitty':'kitty', 'wezterm':'WezTerm'}[self.client]
            candidates = [w for w in candidates if w['kCGWindowOwnerName']==owner]
            if self.client in ('iterm', 'terminal'):
                candidates = [w for w in candidates if str(w['kCGWindowNumber']) == self.id]
            if len(candidates)==1:
                self.window=candidates[0]; break
            if time.monotonic()>deadline: raise RuntimeError('cannot uniquely identify owned GUI window')
            time.sleep(.2)

    def remote(self, *args):
        if self.client=='kitty':
            return command([self.kitty, '@', '--to', 'unix:'+self.socket, *args], stderr=subprocess.DEVNULL)
        environment = dict(os.environ, WEZTERM_UNIX_SOCKET=str(Path.home()/'.local/share/wezterm'/f'gui-sock-{self.gui.pid}'))
        return command([self.wez, 'cli', '--no-auto-start', '--class', self.token, *args], env=environment, stderr=subprocess.DEVNULL)

    def ghost(self, operation, text):
        return apple('tell application id "com.mitchellh.ghostty"\n'+operation+'\nend tell', self.id, text)

    def screen(self):
        if self.client=='ghostty':
            script='on run argv\ntell application id "com.mitchellh.ghostty" to perform action "write_screen_file:copy,plain" on terminal id (item 1 of argv)\nend run'
            path=command([self.helper, 'clipboard-command', '/usr/bin/osascript', '-e', script, self.id])
            return Path(path).read_text()
        if self.client=='iterm':
            self.rows=int(apple('tell application id "com.googlecode.iterm2" to return rows of current session of window id (item 1 of argv as integer)', self.id))
            return visible_tail(apple('tell application id "com.googlecode.iterm2" to return contents of current session of window id (item 1 of argv as integer)', self.id), self.rows)
        if self.client=='terminal':
            return apple('tell application id "com.apple.Terminal" to return contents of selected tab of window id (item 1 of argv as integer)', self.id)
        if self.client=='kitty': return self.remote('get-text', '--match', 'id:'+self.id)
        return self.remote('get-text', '--pane-id', self.id)

    def text(self, value):
        if self.client=='ghostty': return self.ghost('input text (item 2 of argv) to terminal id (item 1 of argv)', value)
        if self.client=='iterm':
            return apple('tell application id "com.googlecode.iterm2" to tell current session of window id (item 1 of argv as integer) to write text (item 2 of argv) newline false', self.id, '\x1b[200~'+value+'\x1b[201~')
        if self.client=='terminal':
            return apple('tell application id "com.apple.Terminal" to do script (item 2 of argv) in selected tab of window id (item 1 of argv as integer)', self.id, value)
        if self.client=='kitty': return self.remote('send-text', '--match', 'id:'+self.id, '--bracketed-paste', 'enable', value)
        return self.remote('send-text', '--pane-id', self.id, value)

    def raw(self, value):
        """Send terminal input without bracketed-paste wrapping (for search keys)."""
        if self.client == 'kitty':
            return self.remote('send-text', '--match', 'id:'+self.id, '--bracketed-paste', 'disable', value)
        if self.client == 'terminal':
            return self.text(value)
        for character in value:
            self.key(character)

    def key(self, key):
        if self.client=='ghostty':
            if len(key)==1:
                return self.ghost('perform action (item 2 of argv) on terminal id (item 1 of argv)', 'text:'+key)
            return self.ghost('send key (item 2 of argv) to terminal id (item 1 of argv)', key.lower())
        values={'F2':'\x1bOQ','Enter':'\r','Escape':'\x1b','Tab':'\t','CtrlC':'\x03'}
        value=values.get(key, key)
        if self.client=='kitty': return self.remote('send-key', '--match', 'id:'+self.id, {'Enter':'enter','Escape':'escape','Tab':'tab','F2':'f2','CtrlC':'ctrl+c'}.get(key, key))
        if self.client=='wezterm': return self.remote('send-text', '--no-paste', '--pane-id', self.id, value)
        if self.client=='iterm':
            return apple('tell application id "com.googlecode.iterm2" to tell current session of window id (item 1 of argv as integer) to write text (item 2 of argv) newline false', self.id, value)
        return self.text(value)

    def submit(self, text):
        self.text(text)
        if self.client!='terminal': self.key('Enter')

    def resize(self, narrow=False):
        cols, rows = (90,30) if narrow else (120,40)
        if self.client=='iterm':
            apple('tell application id "com.googlecode.iterm2"\nset columns of current session of window id (item 1 of argv as integer) to (item 2 of argv as integer)\nset rows of current session of window id (item 1 of argv as integer) to (item 3 of argv as integer)\nend tell', self.id,str(cols),str(rows))
        elif self.client=='terminal':
            apple('tell application id "com.apple.Terminal"\nset number of columns of selected tab of window id (item 1 of argv as integer) to (item 2 of argv as integer)\nset number of rows of selected tab of window id (item 1 of argv as integer) to (item 3 of argv as integer)\nend tell',self.id,str(cols),str(rows))
        elif self.client=='kitty': self.remote('resize-os-window','--match','id:'+self.id,'--unit','cells','--width',str(cols),'--height',str(rows))
        elif self.client=='ghostty':
            self.ghost('perform action (item 2 of argv) on terminal id (item 1 of argv)', 'set_font_size:'+('18' if narrow else '12'))
            if narrow:self.notes.append('Ghostty viewport resize exercised by font zoom, not a dragged window edge.')
        elif narrow:
            self.split_id=self.remote('split-pane','--pane-id',self.id,'--right','--percent','30','--','/bin/sleep','120')
            self.remote('activate-pane','--pane-id',self.id)
            self.notes.append('WezTerm viewport resize exercised by creating an owned split, not dragging a window edge.')
        time.sleep(.25)

    def screenshot(self, path):
        command(['/usr/sbin/screencapture','-x','-o','-l'+str(self.window['kCGWindowNumber']),path])


def run_trial(client, bundle, helper, output, usability=False):
    output.mkdir(parents=True, exist_ok=False)
    metadata=verify_bundle(bundle)
    driver_source=Path(__file__).read_bytes()
    (output/'driver.py').write_bytes(driver_source)
    driver=NativeClient(client,bundle,helper,output)
    ledger={'client':client,'binary_sha256':metadata['binary_sha256'],'source_revision':metadata['revision'],
            'driver_sha256':digest(output/'driver.py'), 'helper_sha256':digest(helper),'started':time.time(), 'captures':[], 'actions':[], 'usability_checks':usability, 'passed':False}
    before=set((bundle/'runs').iterdir())
    def wait(marker):
        if marker == 'ready · idle' and metadata.get('tui') == 'fullscreen' and metadata.get('ui') == 'full':
            marker = '⏎ send'
        deadline=time.monotonic()+30
        while True:
            screen=driver.screen()
            if marker in screen:return screen
            if time.monotonic()>deadline:raise RuntimeError('current view missing '+marker)
            time.sleep(.15)
    def capture(name):
        print(client+': '+name, flush=True)
        p=output/(name+'.txt');p.write_text(driver.screen())
        screenshot=output/(name+'.png');driver.screenshot(screenshot)
        ledger['captures'].append({'name':name,'time':time.time(),'text_sha256':digest(p),'png_sha256':digest(screenshot), 'window_geometry':[w['kCGWindowBounds'] for w in driver.windows() if w['kCGWindowNumber']==driver.window['kCGWindowNumber']]})
    def step(name, fn):
        ledger['actions'].append({'name':name,'time':time.time()});fn()
    try:
        driver.launch();ledger['window']=driver.window;ledger['native_target']=driver.id;ledger['native_session']=driver.session_id
        wait('ready · idle' if metadata.get('tui') == 'inline' else 'Ready for first turn');driver.resize();capture('01-ready')
        if client=='terminal':
            driver.text('\x1bOQ');wait('Details');capture('02-detail')
            driver.text('\x1bOQ');driver.submit('native first');wait('TUI_FIXTURE_REPLY_1')
            driver.text('\x1bOQ\t');wait('No active work');capture('03-work');driver.text('\x1bOQ')
        else:
            step('paste multiline draft',lambda:driver.text('native first\npasted second line'))
            wait('pasted second line')
            driver.key('F2');wait('Project browser');capture('02-project')
            if usability:
                driver.raw('/zzzz');wait('No matching rows');capture('02-search-empty')
                driver.key('Enter');wait('No matching rows')
                driver.raw('\x7f'*4+'current');wait('filter: current');capture('02-search-match')
            driver.key('Enter');wait('Details');capture('03-detail')
            driver.key('Escape');wait('Tab tabs')
            if usability:
                driver.key('Escape')  # Search -> browse, retaining filter.
                driver.key('Escape')  # Clear filter, retaining browser.
            driver.key('Tab');wait('No active work');capture('04-work')
            driver.key('Escape');wait('native first');driver.key('Enter');wait('TUI_FIXTURE_REPLY_1')
        wait('ready · idle');driver.submit('native second');wait('TUI_FIXTURE_REPLY_2');wait('ready · idle')
        driver.resize(True);capture('05-resize')
        if usability: wait('⏎ send')
        if client=='wezterm':
            driver.remote('kill-pane','--pane-id',driver.split_id)
            time.sleep(.25);capture('05-resize-restored')
        driver.submit('/session-export scrollback');wait('Transcript printed');capture('06-native-print-return')
        driver.submit('native permission probe')
        if client=='terminal':driver.text('\x1bOQ\t')
        else:driver.key('F2');wait('Project browser');driver.key('Tab')
        wait('Permission required');capture('07-permission')
        if usability:
            current=driver.screen()
            for key in ('[y]', '[a]', '[Shift+A]', '[n]'):
                if current.count(key)!=1: raise RuntimeError('permission choice missing or repeated: '+key)
            if '[n] deny' not in current: raise RuntimeError('deny label clipped')
        driver.key('n');wait('No active work');capture('08-return-work')
        driver.key('F2' if client=='terminal' else 'Escape');wait('TUI_FIXTURE_REPLY_4');wait('ready · idle');capture('09-completed')
        driver.submit('/quit');wait('Press Enter to close this trial');capture('10-exit')
        runs=[p for p in (bundle/'runs').iterdir() if p not in before and p.name.startswith(client+'-')]
        if len(runs)!=1:raise RuntimeError('trial recording is ambiguous')
        ledger['recording']=str(runs[0]);manifest=json.loads((runs[0]/'manifest.json').read_text())
        if not trial_outcome(manifest):raise RuntimeError('fixture outcome failed: '+json.dumps(manifest))
        ledger['passed']=True
    except Exception as error:
        ledger['error']=str(error)
        if driver.id is not None:
            try:capture('failure')
            except Exception as capture_error:ledger['capture_error']=str(capture_error)
    finally:
        # A failed native probe must not leave a permission prompt waiting behind.
        try:
            runs=[p for p in (bundle/'runs').iterdir() if p not in before and p.name.startswith(client+'-')]
            if len(runs)==1:
                ledger['recording']=str(runs[0])
                identity=runs[0]/'process.json'
                if not ledger['passed'] and identity.exists():
                    ident=json.loads(identity.read_text())
                    actual=subprocess.run(['ps','-p',str(ident['pid']),'-o','command='],capture_output=True,text=True,timeout=5).stdout
                    if actual.startswith(ident['executable']+' ') or actual.startswith(str(bundle/'omegon')+' '):
                        cleanup_tree(ident['pid'])
        except Exception as error:
            ledger['passed'] = False
            ledger['process_cleanup_error'] = str(error)
        try:
            driver.close()
            ledger['window_cleanup'] = 'closed or already absent'
        except Exception as error:
            ledger['passed'] = False
            ledger['window_cleanup'] = 'failed'
            ledger['cleanup_error'] = str(error)
        ledger['notes']=driver.notes
        ledger['finished']=time.time()
        (output/'native-trial.json').write_text(json.dumps(ledger,indent=2)+'\n')
    print(json.dumps({'client':client,'passed':ledger['passed'],'error':ledger.get('error'),'output':str(output)}),flush=True)
    return ledger


def main(argv=None):
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--bundle',type=Path,required=True)
    p.add_argument('--helper',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--usability', action='store_true', help='Verify functional search, unique permission choices and narrow send hints')
    p.add_argument('--clients',nargs='+',choices=['ghostty','iterm','kitty','wezterm','terminal'],required=True)
    p.add_argument('--interactive-gui', action='store_true', help='Explicitly open native windows; use only during a dedicated compatibility session')
    args=p.parse_args(argv)
    if not args.interactive_gui:
        p.error('Native trials open GUI windows. Use scripts/tui_acceptance.py for routine headless testing; --interactive-gui explicitly selects disruptive native compatibility testing.')
    args.output.mkdir(parents=True,exist_ok=False)
    results=[]
    for client in args.clients:
        result=run_trial(client,args.bundle.resolve(),args.helper.resolve(),args.output/client,args.usability)
        results.append(result)
        if result.get('window_cleanup') == 'failed' or result.get('process_cleanup_error'):
            break
    (args.output/'summary.json').write_text(json.dumps(results,indent=2)+'\n')
    raise SystemExit(0 if all(r['passed'] for r in results) else 1)

if __name__=='__main__':main()
