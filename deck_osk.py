#!/usr/bin/env python3
"""Teclado fantasma para Steam Deck.

Herda o teclado na tela do sc-controller e o torna invisivel em repouso, revelando
apenas as teclas ao redor do dedo. Nada do pacote sc-controller e modificado.
"""
import time

# Antes de tudo, e de proposito: os imports abaixo custam ~550ms (gi/Gtk ~194,
# scc ~349), e sao a maior parte do tempo entre apertar o atalho e o teclado
# aparecer. Com _T0 depois deles, o diario media so os 158ms restantes e dava a
# impressao de que o custo estava na nossa logica.
_T0 = time.monotonic()

import os
import signal
import subprocess
import sys

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

import layout_sistema  # noqa: E402
from teclado import CONFIG_PATH, TecladoWidget, load_config  # noqa: E402

PID_PATH = os.path.expanduser("~/.config/scc/ghost-osk.pid")
# Gravado pelo deck-lock enquanto a tela de bloqueio esta no ar.
PID_LOCK = os.path.expanduser("~/.config/scc/deck-lock.pid")
COOLDOWN_PATH = os.path.expanduser("~/.config/scc/ghost-osk.cooldown")
COOLDOWN_S = 1.0
LOG_PATH = os.path.expanduser("~/.config/scc/ghost-osk.log")


def journal(msg: str) -> None:
	"""Diario do ciclo de vida: distingue relancamento de ressurreicao.

	O milesimo e o tempo desde o inicio do processo importam: entre apertar o
	atalho e o teclado assumir os pads o cursor ainda se move, e so medindo da
	para saber se o custo esta no import, na conexao com o daemon ou na janela.
	"""
	try:
		ms = (time.monotonic() - _T0) * 1000
		with open(LOG_PATH, "a") as f:
			f.write(
				f"{time.strftime('%H:%M:%S')}.{int(time.time() * 1000) % 1000:03d} "
				f"pid={os.getpid():<7} [+{ms:7.1f}ms] {msg}\n"
			)
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
		# Modificadores presos pelas teclas do layout, nos dois modos: clique do
		# mouse ou clique do pad. Os grips fisicos passam pelo mapper da base.
		self._mods_ativos = set()
		# Duplo clique no shift trava (caps). Pelo pad nao ha evento de duplo
		# clique do GTK - o clique vem do controle -, entao a deteccao e por
		# tempo, e vale igual no mouse para os dois modos se comportarem igual.
		self._shift_travado = False
		self._ultimo_toque_shift = 0.0
		# Tecla sob o ponteiro no modo mouse. Guardada porque o realce e
		# recalculado do zero a cada mudanca de modificador, e sem isto o
		# hover sumiria ao ligar ou desligar o shift.
		self._sob_ponteiro = None
		# Quem hospeda o teclado pode querer refletir o estado fora dele - a
		# tela de bloqueio acende o aviso de Caps Lock com isto.
		self._ao_modificar = None
		# A base poe o AltGr em MOD1, que e o Alt comum: com essa mascara o
		# nivel 3 nunca entrava na traducao e a tecla nao mudava rotulo nenhum.
		# AltGr e ISO_Level3_Shift, que no X e MOD5.
		from scc.actions import Keys

		self.MODIFIER_MASKS = dict(Keyboard.MODIFIER_MASKS)
		self.MODIFIER_MASKS[Keys.KEY_RIGHTALT] = Gdk.ModifierType.MOD5_MASK
		Keyboard.__init__(self, config)
		self._make_transparent()
		# Em Wayland a base fixa o grupo em 0 e nunca mais mexe. Sem isto o
		# teclado fica em us para sempre, mesmo com o sistema em br.
		self.sincronizar_layout()
		self._watch_layout = layout_sistema.observar_layout(
			lambda descricao: self.sincronizar_layout(alinhar=True, descricao=descricao),
		)

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

	def definir_ao_modificar(self, callback) -> None:
		"""Avisa o hospedeiro sempre que um modificador muda de estado."""
		self._ao_modificar = callback

	def definir_modificador(self, tecla, ativo: bool) -> None:
		"""Liga/desliga um modificador preso pelas teclas do layout.

		No caminho do grip fisico isto nao existe: o grip ja pressiona o shift
		no mapper, e a base inclui esses modificadores no calculo dos rotulos.
		Clicando a tecla desenhada o estado tem de ser nosso - e entrar no MESMO
		calculo, para as teclas mostrarem ! @ # em vez de 1 2 3.
		"""
		if ativo:
			self._mods_ativos.add(tecla)
		else:
			self._mods_ativos.discard(tecla)
		self.update_labels()
		if self._ao_modificar is not None:
			self._ao_modificar()

	def update_labels(self) -> None:
		"""Rotulos conforme o layout ativo do sistema e os modificadores presos.

		Nao chama a base nem quando nao ha modificador nosso: ela le o estado
		so de mapper.keyboard._pressed, que o modo mouse nao tem, e devolve
		rotulo vazio para tecla morta. O calculo e o mesmo dela - e o
		translate_keyboard_state que resolve o simbolo -, com as duas fontes
		de modificador somadas e o acento desenhado quando aparece.
		"""
		if self.background is None:
			return          # antes de _create_background: nao ha o que rotular

		from scc.actions import Keys
		from scc.gui.keycode_to_key import KEY_TO_KEYCODE
		from scc.osd.keyboard import SPECIAL_KEYS

		mt = Gdk.ModifierType(self.keymap.get_modifier_state())
		for tecla in self._mods_ativos:
			mt |= self.MODIFIER_MASKS.get(tecla, Gdk.ModifierType(0))
		# Os grips fisicos seguram o modificador no proprio mapper, e nao no
		# nosso conjunto: sem somar os dois, segurar o grip nao trocaria os
		# rotulos - que e o que a base faz e nao podemos perder.
		if self.mapper is not None:
			for tecla in self.mapper.keyboard._pressed:
				mt |= self.MODIFIER_MASKS.get(tecla, Gdk.ModifierType(0))

		labels = {}
		for button in self.background.buttons:
			if getattr(Keys, button.name, None) not in KEY_TO_KEYCODE:
				continue
			keycode = KEY_TO_KEYCODE[getattr(Keys, button.name)]
			t = self.keymap.translate_keyboard_state(keycode, mt, self.group)
			keyval = t.keyval if hasattr(t, "keyval") else t[1]
			code = Gdk.keyval_to_unicode(keyval)
			if code >= 33:
				labels[button] = chr(code).strip()
			else:
				# Acento morto nao tem unicode proprio: keyval_to_unicode
				# devolve 0 e a tecla apareceria em branco. No br sao oito
				# posicoes, entre elas o til e o circunflexo.
				acento = layout_sistema.acento_morto(keyval)
				labels[button] = acento[0] if acento else SPECIAL_KEYS.get(code)
		self.background.set_labels(labels)
		self._rotular_modificadores()

	def acento_morto(self, nome_tecla: str) -> str | None:
		"""O combinante do acento nesta tecla, se for uma tecla morta.

		Quem digita no campo da tela de bloqueio precisa saber: la o caractere
		entra direto no Gtk.Entry, sem passar pelo compositor, entao a
		composicao com a proxima tecla tem de ser feita a mao.
		"""
		from scc.actions import Keys
		from scc.gui.keycode_to_key import KEY_TO_KEYCODE

		tecla = getattr(Keys, nome_tecla, None)
		if tecla not in KEY_TO_KEYCODE:
			return None
		mt = Gdk.ModifierType(self.keymap.get_modifier_state())
		for t in self._mods_ativos:
			mt |= self.MODIFIER_MASKS.get(t, Gdk.ModifierType(0))
		t = self.keymap.translate_keyboard_state(KEY_TO_KEYCODE[tecla], mt, self.group)
		keyval = t.keyval if hasattr(t, "keyval") else t[1]
		acento = layout_sistema.acento_morto(keyval)
		return acento[1] if acento else None

	def sincronizar_layout(self, alinhar: bool = False, descricao: str = "") -> None:
		"""Adota o layout ativo do sistema.

		Com alinhar, poe tambem os teclados virtuais do sc-controller no mesmo
		grupo. Isso so importa quando ha uinput - no modo pad -, e e o que
		impede o teclado de mostrar uma tecla e o compositor escrever outra.
		"""
		# A descricao vem do evento do compositor e diz de qual layout se
		# trata. Vale mais do que perguntar qual e o ativo: com o teclado
		# aberto, quem responde a essa pergunta e o nosso proprio dispositivo.
		atual = layout_sistema.indice_de(descricao) if descricao else None
		if atual is None:
			atual = layout_sistema.grupo_ativo()
		if atual is None:
			# Sem leitura confiavel, o grupo que ja temos e a melhor resposta -
			# mas o alinhamento tem de acontecer mesmo assim, senao o teclado
			# virtual recem-criado fica num grupo diferente do que esta escrito
			# nas teclas.
			indice, codigo = self.group, "grupo atual"
		else:
			indice, codigo = atual
		if alinhar:
			mudados = layout_sistema.alinhar_teclados_virtuais(indice)
			if mudados:
				journal(f"teclado virtual alinhado em {codigo}: {', '.join(mudados)}")
		if indice != self.group:
			journal(f"layout do sistema: grupo {self.group} -> {indice} ({codigo})")
			self.group = indice
		self.update_labels()

	def _rotular_modificadores(self) -> None:
		"""Shift e AltGr nao produzem caractere, entao a traducao nao devolve
		rotulo nenhum e elas apareciam como retangulos vazios. O nome vai a mao,
		e a tecla ativa entra no realce - assim o estado se le na propria tecla,
		sem indicador separado para manter em sincronia."""
		nomes = {"KEY_LEFTSHIFT": "\u21e7", "KEY_RIGHTALT": "Alt"}
		for b in self.background.buttons:
			if b.name in nomes:
				b.label = nomes[b.name]
		self._aplicar_realce()

	def soltar_modificadores(self) -> None:
		"""Solta o que ficou preso por uinput.

		Um shift travado fica fisicamente pressionado no teclado virtual. Se o
		teclado sumir sem soltar, a sessao inteira herda um shift eterno - e
		nao ha tecla na tela para desfaze-lo.
		"""
		if not self._mods_ativos:
			return
		presos = tuple(self._mods_ativos)
		if self.mapper is not None:
			self.mapper.keyboard.releaseEvent(list(presos))
		self._mods_ativos.clear()
		self._shift_travado = False
		journal(f"modificadores soltos: {len(presos)}")
		self.update_labels()
		if self._ao_modificar is not None:
			self._ao_modificar()

	def _realce_atual(self) -> set:
		"""Todo o realce, montado do zero.

		Os modificadores ativos ficam acesos por conta propria, e o cursor
		(dedo no pad ou ponteiro do mouse) acende a tecla sob ele. Precisa ser
		um calculo unico porque o hilight() da base sobrescreve o conjunto
		inteiro: quem so acrescentasse perderia o realce no proximo movimento.
		"""
		from scc.actions import Keys

		realce = {
			b for b in self.background.buttons
			if getattr(Keys, b.name, None) in self._mods_ativos
		}
		realce.update(b for b in self._hovers.values() if b)
		if self._sob_ponteiro is not None:
			realce.add(self._sob_ponteiro)
		return realce

	def _aplicar_realce(self, pressed=None) -> None:
		if pressed is None:
			pressed = self.background._pressed
		self.background.hilight(self._realce_atual(), pressed)
		self.background.queue_draw()

	def update_background(self, *a) -> None:
		"""Mesma via da base, somando os modificadores presos."""
		self._aplicar_realce({x for x in self._pressed_areas.values() if x})

	DUPLO_CLIQUE_S = 0.6
	MODIFICADORES = ("KEY_LEFTSHIFT", "KEY_RIGHTALT")

	def alternar_modificador(self, nome: str) -> None:
		"""Liga, trava ou desliga um modificador desenhado no layout.

		Um toque liga (vale a proxima tecla); dois toques rapidos travam, como
		o caps de teclado de toque; com ele travado, um toque desliga.
		"""
		from scc.actions import Keys

		tecla = getattr(Keys, nome)
		agora = time.monotonic()
		if nome == "KEY_LEFTSHIFT":
			duplo = (agora - self._ultimo_toque_shift) < self.DUPLO_CLIQUE_S
			self._ultimo_toque_shift = agora
			if self._shift_travado:
				self._shift_travado = False
				ativo = False
			elif duplo and tecla in self._mods_ativos:
				self._shift_travado = True
				ativo = True
			else:
				ativo = tecla not in self._mods_ativos
		else:
			ativo = tecla not in self._mods_ativos

		travado = self._shift_travado and nome == "KEY_LEFTSHIFT"
		journal(f"{nome}: {'travado' if travado else ('ligado' if ativo else 'desligado')}")
		self.definir_modificador(tecla, ativo)
		# O teclado virtual precisa estar com a tecla presa de verdade, senao o
		# caractere sai sem o modificador.
		if self.mapper is not None:
			if ativo:
				self.mapper.keyboard.pressEvent([tecla])
			else:
				self.mapper.keyboard.releaseEvent([tecla])

	@property
	def shift_travado(self) -> bool:
		"""Caps ligado pelo duplo clique. Quem mostra o aviso na tela precisa saber."""
		return self._shift_travado

	def consumir_shift(self) -> None:
		"""Shift simples vale uma tecla; travado (caps) fica."""
		from scc.actions import Keys

		if self._shift_travado or Keys.KEY_LEFTSHIFT not in self._mods_ativos:
			return
		if self.mapper is not None:
			self.mapper.keyboard.releaseEvent([Keys.KEY_LEFTSHIFT])
		self.definir_modificador(Keys.KEY_LEFTSHIFT, False)

	def key_from_cursor(self, cursor, pressed) -> None:
		"""Shift e AltGr desenhados sao sticky; o resto segue a base.

		Segurar nao serve pelo pad: o dedo que mantem a tecla modificadora e o
		mesmo que precisaria clicar a letra. Nos grips fisicos segurar continua
		valendo - la sao dedos diferentes, e a base cuida disso sozinha.
		"""
		if not pressed:
			Keyboard.key_from_cursor(self, cursor, pressed)
			return
		x, y = cursor.position
		for button in self.background.buttons:
			if button.contains(x, y) and button.name in self.MODIFICADORES:
				self.alternar_modificador(button.name)
				return
		Keyboard.key_from_cursor(self, cursor, pressed)
		self.consumir_shift()

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
		self.soltar_modificadores()
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
		# A base cria o mapper (e o uinput) dentro do show().
		self.timer("layout", 0.1, lambda: self.sincronizar_layout(alinhar=True))
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
		"""Posiciona o cursor do pad usando a area do TECLADO, nao a da janela.

		O metodo da base limita a posicao por self.get_allocation() - a janela
		do teclado. No modo embutido essa janela nunca e mostrada e continua
		1x1, entao o clamp achatava tudo e os cursores ficavam presos em (0,0)
		com o pad respondendo normalmente. Aqui a referencia e o background,
		que esta de fato na tela.
		"""
		from scc.constants import STICK_PAD_MAX, ControllerFlags
		from scc.tools import circle_to_square, clamp

		if cursor not in self._hovers or self._controller is None:
			return
		area = self.background.get_allocation()
		cw = cursor.get_allocation().width
		ch = cursor.get_allocation().height
		w = limit[2] - (cw * 0.5)
		h = limit[3] - (ch * 0.5)
		x = x / float(STICK_PAD_MAX)
		y = y / float(STICK_PAD_MAX) * -1.0
		if self._controller.get_flags() & ControllerFlags.LPAD_RPAD_IS_CIRCLE:
			x, y = circle_to_square(x, y)
		x = clamp(cw * 0.5, (limit[0] + w * 0.5) + x * w * 0.5, area.width - cw)
		y = clamp(ch * 0.5, (limit[1] + h * 0.5) + y * h * 0.5, area.height - ch)

		cursor.position = int(x), int(y)
		self.f.move(cursor, x - cw * 0.5, y - ch * 0.5)
		for botao in self.background.buttons:
			if botao.contains(x, y):
				if botao != self._hovers[cursor]:
					self._hovers[cursor] = botao
					if self._pressed[cursor] is not None:
						self.mapper.keyboard.releaseEvent([self._pressed[cursor]])
						self.key_from_cursor(cursor, True)
					if not self.timer_active("update"):
						self.timer("update", 0.01, self.update_background)
					break
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

	def cursores_fora(self) -> None:
		"""Esconde os cursores dos pads. Chamado APOS o show_all do hospedeiro."""
		if getattr(self, "_esconder_cursores", False):
			for cursor in self.cursors.values():
				cursor.hide()

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
			self.background.definir_escala(float(self.cfg.get("escala_mouse", 0.62)))
			# update_labels() vinha junto do ligar(), que o modo mouse nao chama:
			# sem isto as teclas ficam em branco. Nao depende do daemon.
			self.update_labels()
			self.background.connect("button-press-event", self._clique)
			# Realce sob o ponteiro: no modo fantasma o dedo no pad revela as
			# teclas ao redor; aqui o equivalente e a tecla sob o mouse mudar
			# de cor, para o clique ter a mesma confirmacao visual.
			self.background.connect("motion-notify-event", self._hover)
			self.background.connect("leave-notify-event", self._sair_hover)
			# Nao adianta esconder agora: o show_all() do hospedeiro viria
			# depois e traria os cursores de volta. Marcados para o hospedeiro
			# esconder no momento certo.
			self._esconder_cursores = True
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
		# O clique vem em pixels da tela; as teclas vivem em unidades do SVG.
		cx, cy = self.background.para_svg(evento.x, evento.y)
		for botao in self.background.buttons:
			if botao.contains(cx, cy):
				journal(f"  -> tecla {botao.name}")
				self._ao_teclar(botao.name)
				return True
		journal("  -> nenhuma tecla nessa posicao")
		return False

	def _tecla_sob(self, evento):
		"""Tecla sob o ponteiro, ou None. Mesmo hit-test do clique."""
		cx, cy = self.background.para_svg(evento.x, evento.y)
		return next((b for b in self.background.buttons if b.contains(cx, cy)), None)

	def _hover(self, _widget, evento) -> bool:
		"""Realca a tecla sob o ponteiro, no modo mouse.

		Usa o hilight() da base em vez de mexer no _hilight direto: e a mesma
		via que o modo fantasma usa para o cursor, entao o desenho ja sabe
		pintar com color_hilight.
		"""
		sob = self._tecla_sob(evento)
		if sob is not self._sob_ponteiro:
			self._sob_ponteiro = sob
			self._aplicar_realce()
		return False

	def _sair_hover(self, _widget, _evento) -> bool:
		"""Apaga o realce quando o ponteiro sai do teclado - sem isto a ultima
		tecla ficaria acesa para sempre."""
		if self._sob_ponteiro is not None:
			self._sob_ponteiro = None
			self._aplicar_realce()
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
		# Depois do mapper: e ele quem cria o teclado uinput que precisa
		# entrar no mesmo grupo.
		self.timer("labels", 0.1, lambda: self.sincronizar_layout(alinhar=True))
		journal("ligar() concluido - daemon conectado e mapper pronto")

	def desligar(self) -> None:
		"""Solta o controle e os sinais, sem derrubar o processo hospedeiro."""
		self.soltar_modificadores()
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
		# Mesmo motivo do quit() do teclado de desktop: o STEAM+B e consumido
		# duas vezes. O C fecha aqui e, quando o controle volta ao perfil de
		# desktop com o B ainda pressionado, o shell() dispara --toggle e
		# reabriria. Sem esta marca, esconder pelo STEAM+B so pisca o teclado.
		touch_cooldown()
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

	journal(f"invocado argv={argv[1:]} (imports ja carregados)")

	if "--toggle" in argv:
		argv.remove("--toggle")
		# O cooldown vem ANTES do desvio para o lock: o STEAM+B que esconde o
		# teclado embutido tambem chega aqui pelo perfil de desktop, e mandar o
		# SIGUSR1 nesse instante reabriria o que acabou de fechar.
		if in_cooldown():
			journal("toggle IGNORADO (cooldown)")
			return 0
		# Com a tela de bloqueio no ar, o teclado tem de aparecer DENTRO dela:
		# uma janela layer-shell propria ficaria escondida atras da superficie
		# de bloqueio. O lock alterna o teclado embutido ao receber o sinal.
		pid_lock = lock_ativo()
		if pid_lock is not None:
			journal(f"lock ativo (pid={pid_lock}) -> SIGUSR1")
			os.kill(pid_lock, signal.SIGUSR1)
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
