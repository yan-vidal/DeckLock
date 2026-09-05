#!/usr/bin/env python3
"""Layout de teclado do sistema: qual esta ativo, e como escrever com ele.

O sc-controller resolve os rotulos com translate_keyboard_state(codigo,
modificadores, GRUPO), e o grupo e o indice do layout dentro do kb_layout
("us,br" -> us=0, br=1). Em X11 ele le esse indice do XKB; em Wayland ele
fixa 0 e nunca mais mexe - por isso o teclado da tela ficava sempre em us,
mesmo com o teclado fisico em br, e o AltGr nao mudava tecla nenhuma (o us
nao tem nivel 3 em nenhuma das nossas teclas; o br tem em 47).

Aqui o indice vem do compositor, que e quem sabe: o Hyprland diz o layout
ativo de cada teclado e avisa por evento quando muda.

O grupo e POR DISPOSITIVO. O teclado virtual por onde a tela digita e um
dispositivo proprio, e comeca sempre no grupo 0: sem alinha-lo junto, o
rotulo diria "c" com cedilha e o compositor escreveria ";".
"""
import os
import re
import subprocess
import xml.etree.ElementTree as ET

# Acentos que nao produzem caractere sozinhos. O primeiro item e o que se
# desenha na tecla; o segundo e o combinante Unicode que compoe com a letra
# seguinte - assim "acento agudo" + "a" vira "a" com acento, como no teclado
# fisico, sem tabela de pares por idioma.
ACENTOS_MORTOS = {
	"dead_acute": ("´", "́"),
	"dead_grave": ("`", "̀"),
	"dead_tilde": ("~", "̃"),
	"dead_circumflex": ("^", "̂"),
	"dead_diaeresis": ("¨", "̈"),
	"dead_cedilla": ("¸", "̧"),
	"dead_caron": ("ˇ", "̌"),
	"dead_breve": ("˘", "̆"),
	"dead_macron": ("¯", "̄"),
	"dead_abovering": ("˚", "̊"),
	"dead_doubleacute": ("˝", "̋"),
	"dead_ogonek": ("˛", "̨"),
	"dead_abovedot": ("˙", "̇"),
	"dead_belowdot": ("․", "̣"),
	"dead_hook": ("ˀ", "̉"),
	"dead_horn": ("ʼ", "̛"),
	"dead_stroke": ("/", "̸"),
}

_por_keyval: dict[int, tuple[str, str]] | None = None


def acento_morto(keyval: int) -> tuple[str, str] | None:
	"""(glifo, combinante) se este keyval for uma tecla morta.

	A busca e pelo NUMERO do keysym, e nao pelo nome: os nomes tem apelidos e
	o Gdk devolve o que quiser entre eles. O til do br, por exemplo, volta
	como "dead_perispomeni" - mesmo keysym que "dead_tilde", nome diferente,
	e procurar por nome deixava a tecla em branco.
	"""
	global _por_keyval
	if _por_keyval is None:
		from gi.repository import Gdk

		_por_keyval = {}
		for nome, par in ACENTOS_MORTOS.items():
			kv = Gdk.keyval_from_name(nome)
			if kv:
				_por_keyval[kv] = par
	return _por_keyval.get(keyval)


REGRAS_XKB = "/usr/share/X11/xkb/rules/evdev.xml"
_descricoes: dict[str, str] | None = None


def descricoes_de_layout() -> dict[str, str]:
	"""Descricao -> codigo ("Portuguese (Brazil)" -> "br").

	O compositor reporta o layout ativo pela descricao; o kb_layout usa o
	codigo. A tabela que liga os dois e a mesma que o X11 usa, entao nao ha
	lista propria aqui para envelhecer - sao 99 layouts neste sistema.
	"""
	global _descricoes
	if _descricoes is None:
		_descricoes = {}
		try:
			for layout in ET.parse(REGRAS_XKB).getroot().iter("layout"):
				item = layout.find("configItem")
				if item is not None:
					_descricoes[item.findtext("description")] = item.findtext("name")
		except (OSError, ET.ParseError):
			pass
	return _descricoes


def _hyprctl(*args: str) -> str:
	try:
		return subprocess.run(
			["hyprctl", *args], capture_output=True, text=True, timeout=3, check=False,
		).stdout
	except (OSError, subprocess.SubprocessError):
		return ""


def _teclados() -> list[dict]:
	import json

	try:
		return json.loads(_hyprctl("devices", "-j")).get("keyboards", [])
	except (ValueError, AttributeError):
		return []


def eh_virtual(nome: str) -> bool:
	"""Teclado criado por nos (uinput do sc-controller), e nao de verdade."""
	return nome.startswith("scc")


def _codigos_configurados() -> list[str]:
	"""Os codigos do kb_layout, na ordem - e a ordem que define o indice."""
	for k in _teclados():
		codigos = [c.strip() for c in (k.get("layout") or "").split(",") if c.strip()]
		if codigos:
			return codigos
	return []


def indice_de(descricao: str) -> tuple[int, str] | None:
	"""(indice, codigo) para a descricao que o compositor reporta."""
	codigo = descricoes_de_layout().get(descricao)
	codigos = _codigos_configurados()
	if codigo is None or not codigos:
		return None
	try:
		return codigos.index(codigo), codigo
	except ValueError:
		return None


def grupo_ativo() -> tuple[int, str] | None:
	"""(indice, codigo) do layout ativo no teclado principal, ou None.

	None quando nao ha como saber - sem Hyprland, ou quando o unico teclado
	"principal" e um dos nossos. Quem chama deve manter o grupo que ja tinha,
	e nao assumir zero: assumir zero e exatamente o defeito que isto conserta.

	O teclado virtual e ignorado de proposito. Ao ser criado ele vira o
	principal aos olhos do compositor, e como nasce no grupo 0 a leitura se
	tornava circular: o teclado perguntava o layout a si mesmo, respondia
	"us" e desfazia o alinhamento que tinha acabado de aplicar.
	"""
	principal = next(
		(k for k in _teclados() if k.get("main") and not eh_virtual(k.get("name", ""))),
		None,
	)
	if principal is None:
		return None
	return indice_de(principal.get("active_keymap"))


def alinhar_teclados_virtuais(indice: int) -> list[str]:
	"""Poe os teclados do sc-controller no mesmo grupo, e diz quais mudaram.

	Sem isto o teclado da tela escreve no layout errado: cada dispositivo tem
	o seu grupo, e um uinput recem-criado nasce no 0.
	"""
	mudados = []
	for k in _teclados():
		nome = k.get("name", "")
		if not eh_virtual(nome):
			continue
		if _hyprctl("switchxkblayout", nome, str(indice)).strip().startswith("ok"):
			mudados.append(nome)
	return mudados


def observar_layout(ao_mudar) -> int | None:
	"""Chama ao_mudar(descricao) quando o layout de um teclado de verdade muda.

	A descricao vai junto de proposito. Enquanto o teclado da tela esta aberto,
	o dispositivo virtual e quem carrega a marca de "principal", e perguntar
	qual e o layout ativo nao devolve resposta util - mas o evento diz de qual
	teclado se trata e para qual layout ele foi.

	O Hyprland emite 'activelayout>>dispositivo,descricao' no socket2. Ler o
	evento evita ficar perguntando de tempos em tempos, e faz a troca aparecer
	na hora, com o teclado ja aberto na tela.

	Devolve o id do watch do GLib, ou None se nao deu para escutar.
	"""
	import socket

	from gi.repository import GLib

	assinatura = os.environ.get("HYPRLAND_INSTANCE_SIGNATURE")
	runtime = os.environ.get("XDG_RUNTIME_DIR")
	if not assinatura or not runtime:
		return None
	caminho = os.path.join(runtime, "hypr", assinatura, ".socket2.sock")
	try:
		s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
		s.connect(caminho)
		s.setblocking(False)
	except OSError:
		return None

	def leu(fonte, condicao):
		if condicao & (GLib.IOCondition.HUP | GLib.IOCondition.ERR):
			return False
		try:
			dados = s.recv(4096).decode("utf-8", "replace")
		except (BlockingIOError, OSError):
			return True
		if not dados:
			return False
		# Os teclados virtuais tambem emitem o evento, quando somos nos que
		# acabamos de alinha-los. Reagir a eles seria um laco.
		for linha in dados.splitlines():
			m = re.match(r"activelayout>>([^,]+),(.*)$", linha)
			if m and not eh_virtual(m.group(1)):
				ao_mudar(m.group(2).strip())
				break
		return True

	return GLib.io_add_watch(s, GLib.PRIORITY_DEFAULT, GLib.IOCondition.IN, leu)
