#!/usr/bin/env python3
"""Teclado fantasma para Steam Deck.

Herda o teclado na tela do sc-controller e o torna invisivel em repouso, revelando
apenas as teclas ao redor do dedo. Nada do pacote sc-controller e modificado.
"""
import os
import signal
import subprocess
import sys
import time

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("Rsvg", "2.0")
gi.require_version("GdkX11", "3.0")

from gi.repository import Gdk, Gtk  # noqa: E402

from scc.constants import SCLeftRight  # noqa: E402
from scc.osd.keyboard import Keyboard  # noqa: E402
from scc.osd.slave_mapper import SlaveMapper  # noqa: E402
from scc.tools import init_logging  # noqa: E402

from teclado import CONFIG_PATH, TecladoWidget, load_config  # noqa: E402

PID_PATH = os.path.expanduser("~/.config/scc/ghost-osk.pid")
# Gravado pelo deck-lock enquanto a tela de bloqueio esta no ar.
PID_LOCK = os.path.expanduser("~/.config/scc/deck-lock.pid")
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


# --- Atalhos compartilhados entre o modo normal e o modo teclado ---------------
# Definidos aqui num lugar so para os dois perfis nao divergirem.

HYPR = "hyprctl dispatch"
WS = os.path.expanduser("~/.config/scripts/deck-ws")
# R4/R5, do mesmo lado do botao "...". A nomenclatura varia entre aparelhos,
# entao os dois grips direitos servem de modificador para levar a janela junto.
GRIPS_DIR = ("RGRIP", "RGRIP2")


def _dpad_dir(n: int, tecla: str) -> str:
	"""DOTS+seta troca de workspace; DOTS+grip direito+seta leva a janela junto.

	Sem o DOTS a seta e so a seta. O name() no default e para o shell nao ser
	confundido com uma condicao: o ModeModifier aceita ShellCommandAction como
	condicao quando ela aparece sem botao antes.
	"""
	sinal = f"+{n}" if n > 0 else str(n)
	nav = f'shell("{WS} {n}")'
	mov = f'shell("{WS} {n} move")'
	cond = "".join(f"{g}, {mov}, " for g in GRIPS_DIR)
	return f'mode(DOTS, mode({cond}name("ws {sinal}", {nav})), button(Keys.{tecla}))'


# cima/baixo pulam 2; direita/esquerda pulam 1
DPAD_ACTION = "dpad({}, {}, {}, {})".format(
	_dpad_dir(2, "KEY_UP"),
	_dpad_dir(-2, "KEY_DOWN"),
	_dpad_dir(-1, "KEY_LEFT"),
	_dpad_dir(1, "KEY_RIGHT"),
)

# STEAM + botao. O valor e o comando; o default de cada botao muda conforme o
# perfil, por isso fica fora daqui. O B nao entra aqui: o que ele faz com o
# STEAM depende do perfil (abre o teclado no normal, fecha no proprio teclado).
ATALHOS_STEAM = {
	"A": "fuzzel",
	"Y": "footclient",
	"X": f"{HYPR} killactive",
}

# STEAM + botao, sem camada de menu. O Handy nao captura atalho global em
# Wayland (so o compositor ve as teclas antes da janela em foco), e pelo
# controle vale a mesma logica: quem dispara e o binding, chamando o CLI que
# conversa com a instancia ja rodando.
HANDY = os.path.expanduser("~/.local/bin/handy")
ATALHOS_SIMPLES = {"RB": f"{HANDY} --toggle-transcription"}

# "..." + botao. O DOTS ja serve de modificador para o d-pad (workspaces),
# entao segurar ele e um gesto que a mao ja conhece.
ATALHOS_DOTS = {"LB": "nemo"}

# L1 + R1: a cola de atalhos. Os dois ombros juntos nao colidem com nada,
# e e um gesto dificil de fazer sem querer.
ATALHOS_LB = {"RB": os.path.expanduser("~/.config/scripts/deck-atalhos")}

# STEAM + grip esquerdo + botao: menus rapidos. Grips esquerdos porque os
# direitos ja servem ao "..." para levar janela entre workspaces.
MENU = os.path.expanduser("~/.config/scripts/deck-menu")
GRIPS_ESQ = ("LGRIP", "LGRIP2")
MENUS = {"A": "wifi", "B": "bluetooth", "Y": "audio", "X": "brilho"}


def camadas(botao: str, cmd_steam: str, padrao: str) -> str:
	"""Tres camadas no mesmo botao: puro, STEAM+botao, STEAM+grip esq+botao.

	O name() no default interno e obrigatorio: o ModeModifier trata um
	ShellCommandAction sem botao antes como se fosse uma condicao.
	"""
	cond = "".join(f'{g}, shell("{MENU} {MENUS[botao]}"), ' for g in GRIPS_ESQ)
	return f'mode(C, mode({cond}name("steam", shell("{cmd_steam}"))), {padrao})'

class GhostKeyboard(Keyboard):
	"""Keyboard que informa a imagem onde os dedos estao e redesenha enquanto se move."""

	def __init__(self, cfg: dict, debug_alpha: bool = False, config=None) -> None:
		self.cfg = cfg
		self._debug_alpha = debug_alpha
		self._lock_retry = False
		self._ao_fechar = None
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

		self.background = TecladoWidget(self.args.image, self.cfg)
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

		# R2/L2 pressionam a tecla sob o cursor do lado correspondente, espelhando
		# o clique do pad. O padrao era LEFTSHIFT no L2 (maiuscula) e LEFTCTRL no R2.
		# R2/L2 com dois papeis: com o dedo no pad do respectivo lado pressionam
		# a tecla sob aquele cursor; sem o dedo viram os botoes do mouse, como no
		# modo normal. profile.triggers e indexado por SCTriggers.LT/RT, nao por
		# SCLeftRight: a chave errada ADICIONA entradas e deixa o antigo valendo.
		from scc.actions import TriggerAction
		from scc.constants import SCLeftRight, SCTriggers
		from scc.osd.osk_actions import OSKPressAction

		for chave, toque, lado, botao in (
			(SCTriggers.LT, SCButtons.LPADTOUCH, SCLeftRight.LEFT, Keys.BTN_RIGHT),
			(SCTriggers.RT, SCButtons.RPADTOUCH, SCLeftRight.RIGHT, Keys.BTN_LEFT),
		):
			self.profile.triggers[chave] = TriggerAction(
				50, ModeModifier(toque, OSKPressAction(lado), ButtonAction(botao)),
			)

		# Os mesmos atalhos do modo normal, para nao mudarem de comportamento
		# quando o teclado esta aberto. O default de cada botao aqui e o que ele
		# ja fazia no teclado (X e Y fecham, A e Enter), entao nada se perde.
		from scc.actions import NoAction
		from scc.constants import SCPads
		from scc.parser import TalkingActionParser
		from scc.special_actions import ShellCommandAction

		from scc.modifiers import NameModifier

		self.profile.pads[SCPads.DPAD] = TalkingActionParser().restart(DPAD_ACTION).parse().compress()

		def _camada_menu(botao: str, com_steam):
			"""STEAM+grip esquerdo abre o menu; so STEAM faz `com_steam`."""
			menu = ShellCommandAction(f"{MENU} {MENUS[botao]}")
			cond = []
			for g in GRIPS_ESQ:
				cond += [getattr(SCButtons, g), menu]
			return ModeModifier(*cond, com_steam)

		for nome, cmd in ATALHOS_STEAM.items():
			btn = getattr(SCButtons, nome)
			original = self.profile.buttons.get(btn) or NoAction()
			interno = _camada_menu(nome, NameModifier("steam", ShellCommandAction(cmd)))
			self.profile.buttons[btn] = ModeModifier(SCButtons.C, interno, original).compress()

		# B: aqui o STEAM fecha o teclado, em vez de abri-lo como no modo normal
		self.profile.buttons[SCButtons.B] = ModeModifier(
			SCButtons.C,
			_camada_menu("B", CloseOSKAction()),
			ButtonAction(Keys.KEY_ESC),
		).compress()

		# L1/R1 iguais nos dois modos: backspace e espaco
		self.profile.buttons[SCButtons.LB] = ButtonAction(Keys.KEY_BACKSPACE)
		self.profile.buttons[SCButtons.RB] = ButtonAction(Keys.KEY_SPACE)

		# Depois do bloco acima, e nao antes: ele define o padrao de L1/R1 e
		# sobrescreveria o atalho, que precisa envolver esse padrao.
		for mod, atalhos in (
			(SCButtons.C, ATALHOS_SIMPLES),
			(SCButtons.DOTS, ATALHOS_DOTS),
			(SCButtons.LB, ATALHOS_LB),
		):
			for nome, cmd in atalhos.items():
				btn = getattr(SCButtons, nome)
				original = self.profile.buttons.get(btn) or NoAction()
				self.profile.buttons[btn] = ModeModifier(
					mod, ShellCommandAction(cmd), original,
				).compress()

		journal("gatilhos: " + " | ".join(
			f"{k.name}={v.describe(0).replace(chr(10), ' / ')}"
			for k, v in self.profile.triggers.items()
		))
		journal("atalhos: " + " | ".join(
			f"{n}={self.profile.buttons[getattr(SCButtons, n)].describe(0).replace(chr(10), ' / ')}"
			for n in ("A", "B", "X", "Y", "LB", "RB")
		) + f" | DPAD={self.profile.pads[SCPads.DPAD].describe(0).replace(chr(10), ' / ')}")
		self.set_help()

	def on_event(self, daemon, what, data) -> None:
		Keyboard.on_event(self, daemon, what, data)
		self._schedule_redraw()


class TecladoEmbutido(GhostKeyboard):
	"""GhostKeyboard cuja janela propria nunca aparece.

	O conteudo e reparentado para dentro de outra janela - na pratica a da
	tela de bloqueio, a unica superficie que o compositor desenha com a
	sessao travada. Toda a logica vem herdada (daemon, lock dos pads,
	perfil, gradiente, gatilhos); muda so quem hospeda os widgets.
	"""

	def _make_transparent(self) -> None:
		"""No modo embutido nao ha janela propria para tornar transparente.

		A versao da base registra CSS no ESCOPO DA TELA e mexe no visual da
		janela; aqui isso vazaria para a janela do hospedeiro, que perde o
		proprio fundo. Quem cuida do fundo e a tela de bloqueio.
		"""

	@staticmethod
	def ha_controle() -> bool:
		"""Ha um controle na mao do daemon do sc-controller?

		Olha os devices virtuais que o daemon cria (SCController Keyboard/Mouse):
		eles so existem enquanto ele esta gerenciando um controle de verdade.

		NAO serve perguntar ao DaemonManager recem-criado: a conexao dele e
		assincrona, e is_alive() responde "nao" no instante seguinte a
		construcao - o que fazia o teclado cair no modo mouse mesmo com o
		controle na mao.
		"""
		try:
			with open("/proc/bus/input/devices", encoding="utf-8") as f:
				return "SCController" in f.read()
		except OSError:
			return False

	def definir_ao_fechar(self, funcao) -> None:
		"""Quem hospeda decide o que 'fechar o teclado' significa."""
		self._ao_fechar = funcao

	def montar(self, ao_teclar=None):
		"""Devolve o conteudo do teclado para o hospedeiro adicionar.

		Com `ao_teclar`, entra no modo mouse: teclado solido e clicavel, para
		quando nao ha controle. O callback recebe o nome da tecla (KEY_A...).
		"""
		if self.background is None:
			self._create_background()
		# Embutido, a DrawingArea divide o buffer com a janela do hospedeiro.
		self.background.limpar_fundo = False
		if ao_teclar is not None:
			self._ao_teclar = ao_teclar
			# Sem pads nao ha gradiente que faca sentido: o teclado tem de
			# estar inteiro visivel para ser clicado.
			self.background.debug_alpha = True
			self.background.connect("button-press-event", self._clique)
			for cursor in self.cursors.values():
				cursor.hide()
		filho = self.c
		self.remove(filho)
		return filho

	def _clique(self, _widget, evento) -> bool:
		"""Descobre a tecla sob o ponteiro. Mesmo hit-test dos cursores."""
		# O GTK manda button-press-event tres vezes num clique duplo: uma
		# BUTTON_PRESS, outra 2BUTTON_PRESS e mais uma. Sem filtrar, uma
		# tecla clicada duas vezes rapido digitava tres letras.
		if evento.type != Gdk.EventType.BUTTON_PRESS:
			return True
		journal(f"clique em ({evento.x:.0f}, {evento.y:.0f})")
		for botao in self.background.buttons:
			if botao.contains(evento.x, evento.y):
				journal(f"  -> tecla {botao.name}")
				self._ao_teclar(botao.name)
				return True
		journal("  -> nenhuma tecla nessa posicao")
		return False

	@staticmethod
	def _liberar_pads() -> None:
		"""Encerra o teclado de desktop, se estiver aberto.

		Ele trava LPAD/RPAD no daemon, e o lock so consegue os pads se
		alguem soltar. Acontece de verdade: com o teclado aberto quando a
		tela bloqueia, o teclado do lock subia inerte com "Cannot lock LPAD".
		"""
		pid = running_pid()
		if pid is None:
			return
		journal(f"encerrando o teclado de desktop (pid={pid}) para liberar os pads")
		try:
			os.kill(pid, signal.SIGTERM)
			time.sleep(1.0)
		except OSError as e:
			journal(f"nao consegui encerrar: {e}")

	def ligar(self) -> None:
		"""Conecta ao daemon e prepara o mapper, sem abrir janela nenhuma.

		Faz o mesmo que show(), menos o OSDWindow.show() - que criaria a
		superficie layer-shell propria, justamente o que nao serve aqui.
		"""
		from scc.gui.daemon_manager import DaemonManager

		self._liberar_pads()
		self.daemon = DaemonManager()
		self._cononect_handlers()
		self.load_profile()
		self.mapper = SlaveMapper(
			self.profile, None, keyboard=b"SCC OSD Keyboard", mouse=b"SCC OSD Mouse",
		)
		self.mapper.set_special_actions_handler(self)
		for lado in (SCLeftRight.LEFT, SCLeftRight.RIGHT):
			self.set_cursor_position(0, 0, self.cursors[lado], self.limits[lado])
		self._sync_cursor_visibility()
		self.timer("labels", 0.1, self.update_labels)

	def desligar(self) -> None:
		"""Solta o controle e os sinais, sem derrubar o processo hospedeiro."""
		try:
			if self.get_controller():
				self.get_controller().unlock_all()
		except Exception as e:  # noqa: BLE001 - nao pode derrubar o lock
			journal(f"unlock_all falhou: {e}")
		for fonte, eid in self._eh_ids:
			try:
				fonte.disconnect(eid)
			except Exception:  # noqa: BLE001
				pass
		self._eh_ids = []

	def quit(self, code: int = -1) -> None:
		"""Fechar o teclado nao pode encerrar o hospedeiro - mas tem de fechar.

		OSK.close() (STEAM+B, X, Y, C) chega aqui. Antes isto so registrava e
		voltava, e o teclado ficava presa na tela sem meio de sair.
		"""
		import traceback

		origem = " <- ".join(
			f"{q.name}:{q.lineno}" for q in reversed(traceback.extract_stack()[-6:-1])
		)
		journal(f"quit({code}) -> escondendo | origem: {origem}")
		if self._ao_fechar is not None:
			self._ao_fechar()


def lock_ativo() -> int | None:
	"""PID da tela de bloqueio, se houver uma no ar."""
	try:
		pid = int(open(PID_LOCK).read().strip())
		with open(f"/proc/{pid}/cmdline", "rb") as f:
			cmd = f.read().decode("utf-8", "replace")
	except (OSError, ValueError):
		return None
	return pid if "deck" in cmd and "lock" in cmd else None


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
		# Com a tela de bloqueio no ar, o teclado tem de aparecer DENTRO dela:
		# uma janela layer-shell propria ficaria escondida atras da superficie
		# de bloqueio. O lock alterna o teclado embutido ao receber o sinal.
		pid_lock = lock_ativo()
		if pid_lock is not None:
			journal(f"lock ativo (pid={pid_lock}) -> SIGUSR1")
			os.kill(pid_lock, signal.SIGUSR1)
			return 0
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
