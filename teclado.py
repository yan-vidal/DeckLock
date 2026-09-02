#!/usr/bin/env python3
"""Widget do teclado fantasma: desenho e hit-test, sem janela propria.

Vive separado do deck_osk.py porque uma janela nao se embute em outra: o
teclado do desktop e uma janela layer-shell, mas na tela de bloqueio ele
precisa ser um widget DENTRO da janela do lock (a unica superficie que o
compositor desenha quando a sessao esta travada). Os dois usam este mesmo
widget, entao gradiente, hit-test e calibracao nao se duplicam.
"""
import json
import os
import sys
from math import hypot

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("Rsvg", "2.0")
gi.require_version("GdkX11", "3.0")

import cairo  # noqa: E402
from gi.repository import Gdk  # noqa: E402

from scc.gui.svg_widget import SVGEditor  # noqa: E402
from scc.osd.keyboard import KeyboardImage  # noqa: E402

CONFIG_PATH = os.path.expanduser("~/.config/scc/ghost-osk.json")
DEFAULTS = {
	"raio": 180,
	"curva": "smoothstep",
	"altura_tela": 0.35,
	"alpha_repouso": 0.15,
	"fps": 60,
	# Quanto o teclado encolhe no modo mouse, para caber abaixo do campo de
	# senha sem empurrar o resto da tela. 1.0 = tamanho do SVG (800x405).
	"escala_mouse": 0.62,
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


class TecladoWidget(KeyboardImage):
	"""KeyboardImage que desenha cada tecla com alpha em funcao da distancia ao dedo."""

	def __init__(self, image, cfg: dict) -> None:
		# Definidos antes do __init__ da base: ela conecta o sinal "draw",
		# e um expose imediato chamaria on_draw antes destes existirem.
		self.cfg = cfg
		self.cursor_points: list[tuple[float, float]] = []
		self.debug_alpha = False
		# Zerar o buffer so faz sentido quando o teclado tem janela propria.
		# Embutido em outra janela, a DrawingArea pinta no MESMO buffer do
		# hospedeiro - e o OPERATOR_SOURCE apaga o fundo dele.
		self.limpar_fundo = True
		# 1.0 desenha no tamanho do SVG (800x405). Menor que isso encolhe o
		# teclado para caber embaixo do campo de senha sem empurrar nada.
		self.escala = 1.0
		KeyboardImage.__init__(self, image)
		# Precisa vir antes de a DrawingArea ser realizada, senao o widget
		# nasce sem mascara de botao e nenhum clique chega ate ele.
		self.add_events(Gdk.EventMask.BUTTON_PRESS_MASK | Gdk.EventMask.BUTTON_RELEASE_MASK)

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

	def definir_escala(self, escala: float) -> None:
		self.escala = escala
		bg = SVGEditor.find_by_id(self.tree, "BACKGROUND")
		w, h = SVGEditor.get_size(bg)
		self.set_size_request(int(w * escala), int(h * escala))
		self.queue_draw()

	def para_svg(self, x: float, y: float) -> tuple[float, float]:
		"""Converte coordenada da tela para a do layout (que nao escala)."""
		return x / self.escala, y / self.escala

	def on_draw(self, self2, ctx) -> None:
		if self.limpar_fundo:
			# Sem isto o fundo da DrawingArea fica opaco quando o teclado tem
			# janela propria. Ver limpar_fundo no __init__.
			ctx.save()
			ctx.set_operator(cairo.OPERATOR_SOURCE)
			ctx.set_source_rgba(0, 0, 0, 0)
			ctx.paint()
			ctx.restore()

		if self.escala != 1.0:
			# Escala o contexto inteiro: as coordenadas das teclas seguem em
			# unidades do SVG, e so o desenho encolhe.
			ctx.scale(self.escala, self.escala)

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
