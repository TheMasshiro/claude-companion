// Companion for claude-mascot on GNOME Wayland, where other apps can neither see nor move
// the mouse pointer.
//  - Reports "x y away_ms" to the mascot: pointer position, and how long since your last
//    keyboard or mouse input (not counting the moves made here).
//  - Moves the pointer when the mascot asks ("move x y"), for its pointer games, but only
//    while you have been away for at least MIN_AWAY_MS and the screen is not locked. The
//    moment you press a key or move the mouse, further moves are refused.
//  - Never clicks, never types.
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const POLL_MS = 33;
// also resend an unchanged report about once a second, so the mascot knows the feed is alive
const HEARTBEAT_TICKS = 30;
// a hard floor, whatever the mascot's own settings say
const MIN_AWAY_MS = 10000;
// input arriving this soon after one of our own moves is taken to be that move
const OWN_MOVE_MS = 8;

const nowMs = () => GLib.get_monotonic_time() / 1000;

export default class MascotCursorFeed extends Extension {
    enable() {
        const dir = GLib.get_user_runtime_dir();
        this._feedPath = GLib.build_filenamev([dir, 'claude-mascot-cursor.sock']);
        this._feedAddress = Gio.UnixSocketAddress.new(this._feedPath);
        this._out = Gio.Socket.new(Gio.SocketFamily.UNIX, Gio.SocketType.DATAGRAM, Gio.SocketProtocol.DEFAULT);
        this._out.set_blocking(false);

        // where the mascot sends "move x y"
        const controlPath = GLib.build_filenamev([dir, 'claude-mascot-control.sock']);
        GLib.unlink(controlPath);
        this._in = Gio.Socket.new(Gio.SocketFamily.UNIX, Gio.SocketType.DATAGRAM, Gio.SocketProtocol.DEFAULT);
        this._in.bind(Gio.UnixSocketAddress.new(controlPath), true);
        this._in.set_blocking(false);
        this._controlPath = controlPath;
        this._inSource = this._in.create_source(GLib.IOCondition.IN, null);
        this._inSource.set_callback(() => {
            this._onCommands();
            return GLib.SOURCE_CONTINUE;
        });
        this._inSource.attach(null);

        this._pointer = Clutter.get_default_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        this._idle = global.backend.get_core_idle_monitor();
        this._movedAt = -Infinity;
        this._activeAt = nowMs() - this._idle.get_idletime();

        this._encoder = new TextEncoder();
        this._decoder = new TextDecoder();
        this._last = '';
        this._quiet = 0;
        this._timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, POLL_MS, () => {
            this._report();
            return GLib.SOURCE_CONTINUE;
        });
    }

    // Milliseconds since your last keyboard or mouse input, ignoring our own pointer moves.
    _away() {
        const now = nowMs();
        const lastInput = now - this._idle.get_idletime();
        if (lastInput > this._movedAt + OWN_MOVE_MS)
            this._activeAt = Math.max(this._activeAt, lastInput);
        return now - this._activeAt;
    }

    _report() {
        const [x, y] = global.get_pointer();
        const away = Math.floor(this._away() / 250) * 250; // coarse, so it only changes now and then
        const msg = `${x} ${y} ${away}`;
        if (msg === this._last && ++this._quiet < HEARTBEAT_TICKS)
            return;
        this._last = msg;
        this._quiet = 0;
        if (!GLib.file_test(this._feedPath, GLib.FileTest.EXISTS))
            return; // mascot not running
        try {
            this._out.send_to(this._feedAddress, this._encoder.encode(msg), null);
        } catch (e) {
            // mascot just quit; try again next time
        }
    }

    _onCommands() {
        for (;;) {
            let bytes;
            try {
                bytes = this._in.receive_bytes(64, 0, null);
            } catch (e) {
                return; // nothing more to read
            }
            if (!bytes || bytes.get_size() === 0)
                return;
            const [cmd, xs, ys] = this._decoder.decode(bytes.toArray()).trim().split(/\s+/);
            const [x, y] = [Number(xs), Number(ys)];
            if (cmd !== 'move' || !Number.isFinite(x) || !Number.isFinite(y))
                continue;
            if (this._away() < MIN_AWAY_MS || Main.sessionMode.isLocked)
                continue; // you are here (or the screen is locked): the pointer is yours
            this._pointer.notify_absolute_motion(GLib.get_monotonic_time(), x, y);
            this._movedAt = nowMs();
        }
    }

    disable() {
        if (this._timer)
            GLib.source_remove(this._timer);
        this._timer = 0;
        this._inSource?.destroy();
        this._inSource = null;
        this._in?.close();
        this._in = null;
        if (this._controlPath)
            GLib.unlink(this._controlPath);
        this._out?.close();
        this._out = null;
        this._pointer = null;
        this._idle = null;
    }
}
