#!/usr/bin/env python3
"""Teclado fantasma para Steam Deck.

Herda o teclado na tela do sc-controller e o torna invisivel em repouso, revelando
apenas as teclas ao redor do dedo. Nada do pacote sc-controller e modificado.
"""
import json
import os
import signal
import subprocess
import sys
import time
from math import hypot

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("Rsvg", "2.0")
gi.require_version("GdkX11", "3.0")

import cairo  # noqa: E402
from gi.repository import Gdk, Gtk  # noqa: E402

from scc.constants import SCLeftRight  # noqa: E402
from scc.osd.keyboard import Keyboard, KeyboardImage  # noqa: E402
from scc.tools import init_logging  # noqa: E402

CONFIG_PATH = os.path.expanduser("~/.config/scc/ghost-osk.json")
PID_PATH = os.path.expanduser("~/.config/scc/ghost-osk.pid")
COOLDOWN_PATH = os.path.expanduser("~/.config/scc/ghost-osk.cooldown")
COOLDOWN_S = 1.0
LOG_PATH = os.path.expanduser("~/.config/scc/ghost-osk.log")


def journal(msg: str) -> None:
	"""Diario do ciclo de vida: distingue relancamento de ressurreicao."""
	try:
		with open(LOG_PATH, "a") as f:
			f.write(f"{time.strftime('%H:%M:%S')} pid={os.getpid():<7} {msg}\n")
	except OSError:
		pass

# O sc-controller pinta #osd-keyboard com osd_colors["background"] (101010, quase preto).
# Registrado acima de PRIORITY_USER para vencer aquele provider.
GHOST_CSS = b"""
#osd-keyboard, #osd-keyboard-container {
	background-color: transparent;
	background-image: none;
	border: none;
	box-shadow: none;
}
"""
DEFAULTS = {
	"raio": 180,
	"curva": "smoothstep",
	"altura_tela": 0.35,
	"alpha_repouso": 0.15,
	"fps": 60,
}


def load_config() -> dict:
	cfg = dict(DEFAULTS)
	try:
		with open(CONFIG_PATH) as f:
			cfg.update(json.load(f))
	except FileNotFoundError:
		pass
	except (ValueError, OSError) as e:
		print(f"aviso: {CONFIG_PATH} ignorado ({e})", file=sys.stderr)
	return cfg


class GhostKeyboardImage(KeyboardImage):
	"""KeyboardImage que desenha cada tecla com alpha em funcao da distancia ao dedo."""

	def __init__(self, image, cfg: dict) -> None:
		# Definidos antes do __init__ da base: ela conecta o sinal "draw",
		# e um expose imediato chamaria on_draw antes destes existirem.
		self.cfg = cfg
		self.cursor_points: list[tuple[float, float]] = []
		self.debug_alpha = False
		KeyboardImage.__init__(self, image)

	def _falloff(self, d: float) -> float:
		"""1.0 no centro do dedo, 0.0 na borda do raio."""
		r = float(self.cfg["raio"])
		if r <= 0 or d >= r:
			return 0.0
		t = 1.0 - (d / r)
		if self.cfg["curva"] == "smoothstep":
			return t * t * (3.0 - 2.0 * t)
		return t

	def _alpha_for(self, x, y, w, h) -> float:
		"""Maior contribuicao entre os dedos encostados - sem divisa dura no meio."""
		if self.debug_alpha:
			return 1.0
		cx, cy = x + w * 0.5, y + h * 0.5
		best = 0.0
		for px, py in self.cursor_points:
			a = self._falloff(hypot(cx - px, cy - py))
			if a > best:
				best = a
		return best

	def on_draw(self, self2, ctx) -> None:
		# Zera o buffer: sem isso o fundo da DrawingArea fica opaco.
		ctx.save()
		ctx.set_operator(cairo.OPERATOR_SOURCE)
		ctx.set_source_rgba(0, 0, 0, 0)
		ctx.paint()
		ctx.restore()

		ctx.select_font_face(self.font_face, 0, 0)
		ctx.set_line_width(self.LINE_WIDTH)
		ctx.set_font_size(48)
		ascent, descent, height, max_x_advance, max_y_advance = ctx.font_extents()

		max_alpha = 0.0
		for button in self.buttons:
			x, y, w, h = button
			alpha = self._alpha_for(x, y, w, h)
			if alpha <= 0.004:            # invisivel: nao gasta desenho
				continue
			max_alpha = max(max_alpha, alpha)

			if button in self._pressed:
				color = self.color_pressed
			elif button in self._hilight:
				color = self.color_hilight
			elif button.dark:
				color = self.color_button2
			else:
				color = self.color_button1

			ctx.set_source_rgba(color[0], color[1], color[2], color[3] * alpha)
			ctx.rectangle(x, y, w, h)
			ctx.fill()

			b = self.color_button1_border
			ctx.set_source_rgba(b[0], b[1], b[2], b[3] * alpha)
			ctx.rectangle(x, y, w, h)
			ctx.stroke()

			if button.label:
				t = self.color_text
				ctx.set_source_rgba(t[0], t[1], t[2], t[3] * alpha)
				x_bearing, y_bearing, width, _, _, _ = ctx.text_extents(button.label)
				ctx.move_to(x + w * 0.5 - width * 0.5 - x_bearing, y + h * 0.5 + height * 0.3)
				ctx.show_text(button.label)
				ctx.stroke()

		if max_alpha > 0.004:
			Gdk.cairo_set_source_pixbuf(ctx, self.overlay.get_pixbuf(), 0, 0)
			ctx.paint_with_alpha(max_alpha)
		else:
			self._draw_idle_hint(ctx)

	def _draw_idle_hint(self, ctx) -> None:
		"""Traco fino na base: confirma que o teclado esta ativo e capturando o controle."""
		a = float(self.cfg["alpha_repouso"])
		if a <= 0.0:
			return
		alloc = self.get_allocation()
		c = self.color_button1_border
		ctx.set_source_rgba(c[0], c[1], c[2], a)
		ctx.rectangle(alloc.width * 0.25, alloc.height - 3, alloc.width * 0.5, 2)
		ctx.fill()


class GhostKeyboard(Keyboard):
	"""Keyboard que informa a imagem onde os dedos estao e redesenha enquanto se move."""

	def __init__(self, cfg: dict, debug_alpha: bool = False, config=None) -> None:
		self.cfg = cfg
		self._debug_alpha = debug_alpha
		self._lock_retry = False
		Keyboard.__init__(self, config)
		self._make_transparent()

	def _make_transparent(self) -> None:
		"""Janela transparente: visual RGBA + CSS acima do provider do sc-controller."""
		screen = self.get_screen()
		visual = screen.get_rgba_visual()
		if visual is None:
			print("aviso: compositor sem visual RGBA; o fundo ficara opaco", file=sys.stderr)
		else:
			self.set_visual(visual)
		self.set_app_paintable(True)
		provider = Gtk.CssProvider()
		provider.load_from_data(GHOST_CSS)
		Gtk.StyleContext.add_provider_for_screen(
			screen, provider, Gtk.STYLE_PROVIDER_PRIORITY_USER + 100,
		)

	def _create_background(self) -> None:
		from scc.constants import SCPads

		self.background = GhostKeyboardImage(self.args.image, self.cfg)
		self.background.debug_alpha = self._debug_alpha
		self.recolor()
		self.limits = {
			SCLeftRight.LEFT: self.background.get_limit("LIMIT_LPAD"),
			SCLeftRight.RIGHT: self.background.get_limit("LIMIT_RPAD"),
			SCPads.CPAD: self.background.get_limit("LIMIT_CPAD"),
		}
		self._pack()

	def _touching_sides(self) -> set:
		"""Lados com o dedo encostado, lidos do bitmask de botoes do mapper."""
		from scc.constants import SCButtons

		mapper = getattr(self, "mapper", None)
		if mapper is None:
			return set()
		b = int(mapper.buttons)
		sides = set()
		if b & int(SCButtons.LPADTOUCH):
			sides.add(SCLeftRight.LEFT)
		if b & int(SCButtons.RPADTOUCH):
			sides.add(SCLeftRight.RIGHT)
		return sides

	def _sync_cursor_points(self) -> None:
		pts = []
		for side in self._touching_sides():
			cursor = self.cursors.get(side)
			pos = getattr(cursor, "position", None) if cursor else None
			if pos:
				pts.append((float(pos[0]), float(pos[1])))
		self.background.cursor_points = pts

	def _redraw_now(self) -> None:
		self._sync_cursor_points()
		self._sync_cursor_visibility()
		self.background.queue_draw()

	def _sync_cursor_visibility(self) -> None:
		"""As bolinhas dos pads sao widgets GTK, nao passam pelo Cairo do on_draw -
		precisam ser escondidas na mao, senao ficam visiveis com o teclado invisivel."""
		touching = self._touching_sides()
		for side, cursor in self.cursors.items():
			cursor.set_visible(side in touching)

	def on_failed_to_lock(self, error) -> None:
		"""Auto-recuperacao do lock dos pads.

		Um OSD preso no scc-osd-daemon (tipicamente um menu que nao fechou
		direito) segura o lock de LPAD/RPAD e impede o teclado de abrir. O
		daemon respawna o osd-daemon automaticamente e a instancia nova nasce
		limpa, entao derruba-lo destrava. Uma tentativa apenas: se falhar de
		novo, e outra causa e o erro original vale.
		"""
		if self._lock_retry:
			journal(f"lock falhou de novo ({error}) - desistindo")
			Keyboard.on_failed_to_lock(self, error)
			return
		self._lock_retry = True
		journal(f"lock falhou ({error}) - derrubando o osd-daemon e tentando de novo")
		try:
			subprocess.run(["pkill", "-f", "scc-osd-daemon"], timeout=3, check=False)
		except (OSError, subprocess.SubprocessError) as e:
			journal(f"pkill falhou: {e}")
		self.timer("relock", 1.5, self.on_daemon_connected)

	def quit(self, code: int = -1) -> None:
		journal(f"quit(code={code}) - fechando")
		# O mesmo aperto de STEAM+B e consumido duas vezes: fecha o teclado
		# aqui e, quando o controle volta ao perfil de desktop com o B ainda
		# pressionado, dispara o shell() que reabriria. O cooldown gravado na
		# ABERTURA nao protege - ja expirou. Tem de ser gravado agora.
		touch_cooldown()
		Keyboard.quit(self, code)

	def show(self, *a) -> None:
		journal("show() - janela aparecendo")
		Keyboard.show(self, *a)
		self._sync_cursor_visibility()
		ls = getattr(self, "layer_shell", None)
		if ls is None:
			return
		# Sem ancora horizontal, o layer-shell centraliza a janela sozinho.
		ls.set_anchor(self, ls.Edge.LEFT, False)
		ls.set_anchor(self, ls.Edge.RIGHT, False)
		ls.set_margin(self, ls.Edge.LEFT, 0)
		ls.set_margin(self, ls.Edge.RIGHT, 0)

	def _schedule_redraw(self) -> None:
		"""Coalesce redraws: o pad reporta a ~87 Hz, nao precisamos redesenhar tudo isso."""
		if not self.timer_active("ghost"):
			self.timer("ghost", 1.0 / float(self.cfg["fps"]), self._redraw_now)

	def set_cursor_position(self, x, y, cursor, limit) -> None:
		Keyboard.set_cursor_position(self, x, y, cursor, limit)
		self._schedule_redraw()

	def load_profile(self) -> None:
		"""Injeta STEAM+B no perfil do teclado.

		Enquanto o teclado esta aberto ele captura o controle e usa o proprio
		perfil, entao o atalho que o abriu (definido no perfil de desktop) nao
		chega aqui. Sem isso, STEAM+B abre mas nao fecha.

		O parser rejeita OSK.close() aninhado em mode(), por isso o modificador
		e montado em Python.
		"""
		Keyboard.load_profile(self)
		from scc.actions import ButtonAction
		from scc.constants import SCButtons
		from scc.modifiers import ModeModifier
		from scc.osd.osk_actions import CloseOSKAction
		from scc.uinput import Keys

		self.profile.buttons[SCButtons.B] = ModeModifier(
			SCButtons.C, CloseOSKAction(), ButtonAction(Keys.KEY_ESC),
		)

		# R2/L2 pressionam a tecla sob o cursor do lado correspondente, espelhando
		# o clique do pad. O padrao era LEFTSHIFT no L2 (maiuscula) e LEFTCTRL no R2.
		# profile.triggers e indexado por SCTriggers.LT/RT, nao por SCLeftRight:
		# usar a chave errada ADICIONA entradas e deixa o mapeamento antigo valendo.
		from scc.actions import TriggerAction
		from scc.constants import SCLeftRight, SCTriggers
		from scc.osd.osk_actions import OSKPressAction

		self.profile.triggers[SCTriggers.LT] = TriggerAction(50, OSKPressAction(SCLeftRight.LEFT))
		self.profile.triggers[SCTriggers.RT] = TriggerAction(50, OSKPressAction(SCLeftRight.RIGHT))

		self.set_help()

	def on_event(self, daemon, what, data) -> None:
		Keyboard.on_event(self, daemon, what, data)
		self._schedule_redraw()


def running_pid() -> int | None:
	"""PID de uma instancia viva, ou None. Confere a cmdline para nao matar
	um processo alheio que herdou um PID reciclado."""
	try:
		pid = int(open(PID_PATH).read().strip())
	except (OSError, ValueError):
		return None
	try:
		with open(f"/proc/{pid}/cmdline", "rb") as f:
			cmdline = f.read().decode("utf-8", "replace")
	except OSError:
		return None
	return pid if "deck" in cmdline and "osk" in cmdline else None


def in_cooldown() -> bool:
	"""Evita que dois disparos seguidos do botao fechem e reabram o teclado."""
	try:
		return (time.time() - os.path.getmtime(COOLDOWN_PATH)) < COOLDOWN_S
	except OSError:
		return False


def touch_cooldown() -> None:
	try:
		with open(COOLDOWN_PATH, "w"):
			pass
	except OSError:
		pass


def _on_signal(signum, frame):
	"""SIGTERM vem do --toggle. Sem isto o processo morre deixando o pidfile
	orfao, e o indicador da waybar passa a mentir."""
	journal(f"recebeu sinal {signum} - encerrando")
	try:
		os.unlink(PID_PATH)
	except OSError:
		pass
	sys.exit(0)


def main() -> int:
	signal.signal(signal.SIGINT, _on_signal)
	signal.signal(signal.SIGTERM, _on_signal)
	argv = list(sys.argv)
	debug_alpha = "--debug-alpha" in argv
	if debug_alpha:
		argv.remove("--debug-alpha")

	journal(f"invocado argv={argv[1:]}")

	if "--toggle" in argv:
		argv.remove("--toggle")
		if in_cooldown():
			journal("toggle IGNORADO (cooldown)")
			return 0
		touch_cooldown()
		pid = running_pid()
		if pid is not None:
			journal(f"toggle -> matando pid={pid}")
			os.kill(pid, signal.SIGTERM)
			return 0
		journal("toggle -> nenhuma instancia, vai abrir")

	init_logging()
	try:
		with open(PID_PATH, "w") as f:
			f.write(str(os.getpid()))
	except OSError as e:
		print(f"aviso: nao consegui gravar {PID_PATH} ({e})", file=sys.stderr)

	k = GhostKeyboard(load_config(), debug_alpha=debug_alpha)
	if not k.parse_arguments(argv):
		return 1
	k.run()
	try:
		os.unlink(PID_PATH)
	except OSError:
		pass
	journal(f"saindo (exit={k.get_exit_code()})")
	return k.get_exit_code()


if __name__ == "__main__":
	sys.exit(main())
