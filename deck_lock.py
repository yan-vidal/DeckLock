#!/usr/bin/env python3
"""Tela de bloqueio do Steam Deck, com teclado virtual pelo controle.

DE ONDE VEIO: adaptado do video_lock.py do projeto home-server/desktop,
que ja resolvia tela de bloqueio de verdade em Python. O que se ganha de
graca: relogio, fundo de foto ou video, tema por cores.css, avatar,
modo ocioso, uma surface por monitor com hotplug e o modo --preview.

O QUE MUDA AQUI: o teclado fantasma entra como widget DENTRO desta
janela. Um teclado em layer-shell nao serve - verificado com captura de
tela durante o bloqueio: as layers continuam existindo, mas a superficie
de lock e composta acima de todas. Dentro da janela do lock nao ha essa
disputa.

--- docstring original ---

Tela de bloqueio com video de fundo, relogio grande e campo de senha.

POR QUE ESCREVER UMA: nenhuma tela de bloqueio pronta toca video.
Verificado uma a uma - swaylock (so imagem estatica, e sem relogio),
gtklock, hyprlock e waylock aceitam imagem de fundo, nenhuma aceita
video.

E POR QUE DA PRA FAZER EM PYTHON: o pacote gtk-session-lock instala
GtkSessionLock-0.1.typelib, ou seja, binding GObject Introspection. Uma
tela de bloqueio de verdade (protocolo ext-session-lock-v1) e um app GTK
normal que registra suas janelas no compositor por essa biblioteca.
O video entra via GStreamer com "gtksink", que desenha dentro de um
widget GTK como qualquer outro.

    video_lock.py            bloqueia de verdade
    video_lock.py --preview  MESMA tela, porem como janela comum

O --preview existe por seguranca e nao e detalhe: uma tela de bloqueio
com defeito prende a sessao, e a unica saida vira "docker exec" de fora.
Ajuste o visual sempre no preview; so depois arme o bloqueio real.

A midia vem de ~/.config/midias/bloqueio/ (fotos ou videos).
As cores saem do tema (cores.css), como no menu iniciar.
"""
import getpass
import os
import signal
import subprocess
import sys
import random

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gst", "1.0")
gi.require_version("GtkLayerShell", "0.1")
from gi.repository import Gtk, Gdk, GdkPixbuf, GLib, Gst, GtkLayerShell  # noqa: E402

# O teclado mora ao lado deste arquivo. Executado pelo symlink em
# ~/.local/bin, o Python resolve o link antes de definir sys.path[0], entao
# o diretorio real do projeto ja esta no caminho.
from deck_osk import TecladoEmbutido  # noqa: E402
from teclado import load_config  # noqa: E402

# MODO DE AUTENTICACAO ISOLADO - precisa vir antes de qualquer import
# pesado, e e chamado pela propria tela num subprocesso.
#
# POR QUE NAO AUTENTICAR NO PROCESSO PRINCIPAL: o pam_unix faz fork() do
# helper unix_chkpwd, e fork() num processo com varias threads (o
# GStreamer cria as suas) e uma armadilha classica - o filho herda so a
# thread que chamou, e se outra segurava um lock interno no instante do
# fork, o filho trava e o pai fica esperando em do_wait pra sempre.
# Foi exatamente o que aconteceu: a senha era aceita e a tela congelava
# ao dar Enter.
#
# Aqui o subprocesso nasce por fork+exec (execve limpa o estado), com uma
# thread so e sem GStreamer - o fork do PAM acontece em terreno seguro.
if "--auth" in sys.argv:
    import pam

    usuario = sys.argv[sys.argv.index("--auth") + 1]
    senha = sys.stdin.readline().rstrip("\n")
    autenticador = pam.pam()
    ok = autenticador.authenticate(usuario, senha, service="system-auth")
    if not ok:
        print(autenticador.reason or "PAM recusou a autenticacao", file=sys.stderr)
    sys.exit(0 if ok else 1)

PREVIEW = "--preview" in sys.argv
# Forca o teclado embutido a ficar opaco. Sem isto ele e invisivel em
# repouso (modo fantasma) e nao da para conferir se esta no lugar certo.
DEBUG_ALPHA = "--debug-alpha" in sys.argv
# Forca o teclado clicavel mesmo com controle ligado. Serve para testar o
# modo sem desconectar nada, e para quem prefere o mouse.
FORCA_MOUSE = "--mouse" in sys.argv
# Entra direto no modo ocioso (formulario escondido). So faz sentido com
# --preview: sem isso teria que esperar MINUTOS_BLOQUEIO_ATE_OCIOSO pra
# conferir qualquer ajuste visual dessa tela.
PREVIEW_OCIOSO = "--preview-ocioso" in sys.argv

CONFIG_DIR = os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config"))
CORES_CSS = os.path.join(CONFIG_DIR, "desktop-theme", "cores.css")
THEME_CONF = os.path.join(CONFIG_DIR, "desktop-theme", "theme.conf")
DIR_BLOQUEIO = os.path.join(CONFIG_DIR, "midias", "bloqueio")
DIR_OCIOSO = os.path.join(CONFIG_DIR, "midias", "ocioso")
# Como o deck-osk descobre que ha uma tela de bloqueio no ar: existindo este
# arquivo com um processo vivo, o atalho vira SIGUSR1 em vez de abrir o
# teclado do desktop - que ficaria escondido atras da tela de bloqueio.
PID_LOCK = os.path.join(CONFIG_DIR, "scc", "deck-lock.pid")
EXT_VIDEO = (".mp4", ".mkv", ".webm", ".mov")
EXT_FOTO = (".jpg", ".jpeg", ".png", ".webp", ".avif", ".bmp")
# getpass.getuser() consulta a senha do processo, e nao so a variavel de
# ambiente - com a sessao bloqueada o USER pode nao estar no ambiente.
USUARIO = getpass.getuser()
TAM_AVATAR = 96


def journal_lock(msg: str) -> None:
    """Diario da tela de bloqueio. O log do teclado ja provou seu valor; aqui
    faltava um equivalente para o lock, que roda desacoplado do terminal."""
    import time

    try:
        with open(os.path.join(CONFIG_DIR, "scc", "deck-lock.log"), "a") as f:
            f.write(f"{time.strftime('%H:%M:%S')} pid={os.getpid():<7} {msg}\n")
    except OSError:
        pass


def ler_minutos_bloqueio_ate_ocioso():
    """MINUTOS_BLOQUEIO_ATE_OCIOSO do theme.conf (0 = desliga o modo)."""
    try:
        with open(THEME_CONF, encoding="utf-8") as f:
            for linha in f:
                if linha.startswith("MINUTOS_BLOQUEIO_ATE_OCIOSO="):
                    return max(0, int(linha.split("=", 1)[1].strip()))
    except (OSError, ValueError):
        pass
    return 10

# Sem estas, qualquer regra que use @cor e DESCARTADA pelo GTK quando o
# cores.css do usuario nao existe - regra a regra, em silencio. Foi o que
# escondeu o estilo do campo de senha: #relogio (sem @cor) aplicava e
# #senha (com @rosa_translucido) nao. Ficam antes do tema, que sobrescreve.
CORES_PADRAO = """
@define-color primaria #7aa2f7;
@define-color primaria_escura #3d59a1;
@define-color texto #e6e6e6;
@define-color texto_sobre_rosa #1a1b26;
@define-color fundo_barra rgba(26, 27, 38, 0.45);
@define-color fundo_vidro rgba(20, 21, 30, 0.73);
@define-color rosa_translucido rgba(122, 162, 247, 0.80);
@define-color realce rgba(122, 162, 247, 0.55);
@define-color hover rgba(230, 230, 230, 0.14);
"""

CSS_BASE = """
window, #fundo { background-color: black; }

/* Veu escuro por cima do video: sem ele o texto some nas cenas claras. */
#veu {
    background-image: linear-gradient(to bottom,
                                      rgba(0,0,0,0.45),
                                      rgba(0,0,0,0.15) 40%,
                                      rgba(0,0,0,0.65));
}

#relogio {
    color: #ffffff;
    font-size: 96px;
    font-weight: 300;
    text-shadow: 0 2px 12px rgba(0,0,0,0.8);
}

#data {
    color: #ffffff;
    font-size: 22px;
    text-shadow: 0 2px 10px rgba(0,0,0,0.8);
}

#avatar {
    /* A moldura clara separa a foto do video de fundo, que pode ter
       qualquer cor atras. */
    border: 2px solid rgba(255,255,255,0.55);
    border-radius: 50%;
    margin-bottom: 8px;
}

#usuario {
    color: #ffffff;
    font-size: 20px;
    font-weight: bold;
    text-shadow: 0 2px 10px rgba(0,0,0,0.8);
}

#senha {
    background-color: rgba(255,255,255,0.15);
    color: #ffffff;
    border: 1px solid @rosa_translucido;
    border-radius: 20px;
    padding: 10px 18px;
    min-width: 260px;
    font-size: 16px;
}

#senha:focus { border-color: @primaria; }

/* O GTK3 desenha o texto do placeholder com a cor do estado INSENSITIVE do
   proprio campo - nao ha no CSS um no "placeholder" como no GTK4. Sem isto
   ele sai no cinza do tema e some no fundo preto desta tela. */
#senha:disabled { color: rgba(255,255,255,0.55); }


#energia button {
    background-color: rgba(255,255,255,0.10);
    border: 1px solid rgba(255,255,255,0.20);
    border-radius: 18px;
    padding: 6px;
    margin: 4px;
    color: #ffffff;
}

#energia button:hover { background-color: @realce; }

/* O botao de enviar acompanha o campo de senha: mesma borda, mesmo raio e
   mesmo fundo. Sem contorno proprio ele so parecia um botao no hover, e em
   repouso passava por um icone solto ao lado do campo. */
#enviar {
    background-color: rgba(255,255,255,0.15);
    color: #ffffff;
    border: 1px solid @rosa_translucido;
    border-radius: 20px;
    padding: 10px 14px;
}

#enviar:hover { background-color: @hover; }
#enviar:disabled { opacity: 0.45; }

#capslock {
    color: #ffd166;
    font-size: 13px;
    font-weight: bold;
    text-shadow: 0 2px 8px rgba(0,0,0,0.9);
}

#aviso {
    color: #ffd9e2;
    font-size: 14px;
    text-shadow: 0 2px 8px rgba(0,0,0,0.9);
}
"""


def escolher_midia(categoria_dir):
    """Sorteia UM item de midias/<categoria> e devolve (tipo, caminho)."""
    candidatos = []
    for sub, exts, tipo in (
        ("videos", EXT_VIDEO, "video"),
        ("fotos", EXT_FOTO, "foto"),
    ):
        pasta = os.path.join(categoria_dir, sub)
        if not os.path.isdir(pasta):
            continue
        for nome in os.listdir(pasta):
            if nome.lower().endswith(exts):
                candidatos.append((tipo, os.path.join(pasta, nome)))
    return random.choice(candidatos) if candidatos else (None, None)


class TelaBloqueio(Gtk.Window):
    def __init__(self, ao_desbloquear):
        super().__init__()
        self.ao_desbloquear = ao_desbloquear
        self.pipeline = None
        self.modo_ocioso = False
        self._timer_ocioso_id = None
        self._widget_fundo = None
        self.pilha = None

        self.pilha = Gtk.Overlay()
        self.pilha.set_name("fundo")
        self.add(self.pilha)

        self._widget_fundo = self._montar_video(DIR_BLOQUEIO)
        self.pilha.add(self._widget_fundo if self._widget_fundo is not None else Gtk.Box())

        # O gradiente e a caixa de layout sao DOIS widgets. Sao a mesma coisa
        # na tela do Deck, onde o teclado cobre a largura inteira, mas num
        # monitor largo o teclado ocupa so o meio: encolher a caixa para abrir
        # espaco (ver _compactar) tirava o gradiente da faixa de baixo e
        # aparecia uma costura atravessando a tela, com a foto crua dos lados.
        gradiente = Gtk.Box()
        gradiente.set_name("veu")
        self.pilha.add_overlay(gradiente)

        veu = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.veu = veu
        self.pilha.add_overlay(veu)

        # Relogio grande no topo, credenciais embaixo - a disposicao que
        # Windows e macOS usam.
        # Relogio e data vao JUNTOS numa caixa so. Se a data for empacotada
        # solta no "veu", ela gruda no fim da area do relogio - e no modo
        # ocioso (rodape escondido) essa area passa a ser a tela inteira,
        # jogando a data pra ultima linha de pixels. Medido: relogio y=1042,
        # data terminando em y=1080, zero folga. Agrupados, mover o bloco
        # move os dois. O modo normal fica igualzinho (y=419 nos dois casos).
        self.bloco_relogio = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.bloco_relogio.set_valign(Gtk.Align.END)
        veu.pack_start(self.bloco_relogio, True, True, 0)

        self.relogio = Gtk.Label(label="")
        self.relogio.set_name("relogio")
        self.bloco_relogio.pack_start(self.relogio, False, False, 0)

        self.data = Gtk.Label(label="")
        self.data.set_name("data")
        self.bloco_relogio.pack_start(self.data, False, False, 0)

        self.rodape = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        self.rodape.set_valign(Gtk.Align.CENTER)
        self.rodape.set_margin_top(60)
        self.rodape.set_margin_bottom(80)
        veu.pack_start(self.rodape, True, True, 0)

        self.avatar = self._avatar()
        self.rodape.pack_start(self.avatar, False, False, 0)

        # get_real_name() devolve a string "Unknown" quando o GECOS esta
        # vazio - nao None nem "" - entao um `or` simples nao resolve.
        real = GLib.get_real_name()
        self.rotulo_nome = Gtk.Label(label=USUARIO if not real or real == "Unknown" else real)
        self.rotulo_nome.set_name("usuario")
        self.rodape.pack_start(self.rotulo_nome, False, False, 0)

        self.senha = Gtk.Entry()
        self.senha.set_name("senha")
        self.senha.set_visibility(False)
        self.senha.set_input_purpose(Gtk.InputPurpose.PASSWORD)
        self.senha.set_placeholder_text("Senha")
        # O icone da esquerda mostra/oculta o que foi digitado. Comeca oculto;
        # serve para conferir a senha antes de enviar, util quando se digita
        # pelo controle, onde errar uma tecla e facil e nao da para ver.
        self.senha.set_icon_from_icon_name(
            Gtk.EntryIconPosition.PRIMARY, "view-reveal-symbolic"
        )
        self.senha.set_icon_activatable(Gtk.EntryIconPosition.PRIMARY, True)
        self.senha.set_icon_tooltip_text(
            Gtk.EntryIconPosition.PRIMARY, "Mostrar a senha"
        )
        self.senha.set_halign(Gtk.Align.CENTER)
        self.senha.set_alignment(0.5)
        # Botao de teclado no proprio campo: quem esta sem controle nao tem
        # como dar o atalho, e precisa de um jeito clicavel de abrir.
        self.senha.set_icon_from_icon_name(
            Gtk.EntryIconPosition.SECONDARY, "input-keyboard-symbolic"
        )
        self.senha.set_icon_activatable(Gtk.EntryIconPosition.SECONDARY, True)
        self.senha.set_icon_tooltip_text(
            Gtk.EntryIconPosition.SECONDARY, "Teclado virtual"
        )
        self.senha.connect(
            "icon-press",
            lambda _e, pos, *_: (
                self.alternar_teclado()
                if pos == Gtk.EntryIconPosition.SECONDARY
                else self._alternar_visibilidade()
            ),
        )
        self.senha.connect("activate", self._tentar)

        # Botao de enviar ao lado do campo. Quem esta so com o mouse (sem
        # controle e sem teclado fisico) digita pelo teclado virtual, mas nao
        # tem como dar o Enter - faltava o ultimo passo para entrar so clicando.
        #
        # Fora do campo porque as duas posicoes de icone do Gtk.Entry ja servem:
        # PRIMARY mostra/oculta a senha, SECONDARY abre o teclado. As duas sao
        # justamente o que esse mesmo usuario precisa.
        linha = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        linha.set_halign(Gtk.Align.CENTER)

        # Espacador invisivel do tamanho do botao, a esquerda. Sem ele quem
        # fica centralizado e o conjunto campo+botao, e o campo escorrega para
        # a esquerda do eixo - o avatar e o nome, centralizados na coluna,
        # apareciam tortos em relacao a ele. O SizeGroup mantem os dois com a
        # mesma largura mesmo se o icone ou o padding mudarem.
        espacador = Gtk.Box()
        linha.pack_start(espacador, False, False, 0)
        linha.pack_start(self.senha, False, False, 0)

        self.enviar = Gtk.Button()
        self.enviar.set_name("enviar")
        self.enviar.set_image(
            Gtk.Image.new_from_icon_name("go-next-symbolic", Gtk.IconSize.LARGE_TOOLBAR)
        )
        self.enviar.set_tooltip_text("Entrar")
        # Nasce desligado: com o campo vazio o clique so renderia "senha
        # incorreta", e o botao apagado diz que ainda falta digitar.
        self.enviar.set_sensitive(False)
        self.enviar.connect("clicked", lambda _b: self._tentar(self.senha))
        self.senha.connect(
            "changed", lambda e: self.enviar.set_sensitive(bool(e.get_text()))
        )
        linha.pack_start(self.enviar, False, False, 0)
        grupo = Gtk.SizeGroup(mode=Gtk.SizeGroupMode.HORIZONTAL)
        grupo.add_widget(espacador)
        grupo.add_widget(self.enviar)

        # Caps Lock ligado com a senha oculta e um erro dificil de enxergar:
        # digita-se tudo em maiusculas sem perceber e a senha e recusada sem
        # explicacao. O rotulo so existe na tela quando esta ligado.
        #
        # set_no_show_all e obrigatorio: sem ele o show_all() da janela mostra
        # o aviso sempre, inclusive com o Caps Lock desligado.
        self.capslock = Gtk.Label(label="Caps Lock ligado")
        self.capslock.set_name("capslock")
        self.capslock.set_no_show_all(True)
        self.rodape.pack_start(self.capslock, False, False, 0)

        self.rodape.pack_start(linha, False, False, 0)

        self.keymap = Gdk.Keymap.get_for_display(Gdk.Display.get_default())
        self.keymap.connect("state-changed", self._sync_capslock)
        self._sync_capslock()

        self.aviso = Gtk.Label(label="")
        self.aviso.set_name("aviso")
        self.rodape.pack_start(self.aviso, False, False, 0)

        # Energia no canto de cima: embaixo fica o teclado, e um clique
        # acidental em "desligar" seria caro demais.
        self.pilha.add_overlay(self._barra_energia())

        # Onde o teclado entra quando chamado. Fica ancorado embaixo para
        # nao cobrir o campo de senha, que e o que se precisa enxergar.
        self.teclado = None
        self.modo_mouse = False
        self.caixa_teclado = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.caixa_teclado.set_valign(Gtk.Align.END)
        self.caixa_teclado.set_halign(Gtk.Align.CENTER)
        # Folga para a ultima fileira nao encostar na borda da tela.
        self.caixa_teclado.set_margin_bottom(10)
        self.pilha.add_overlay(self.caixa_teclado)

        self._tique()
        GLib.timeout_add_seconds(1, self._tique)

        # Depois de N minutos parado na tela de bloqueio: some o formulario
        # e troca a midia pra pasta ocioso (ainda trancado). Qualquer
        # tecla/mouse volta o formulario.
        minutos = ler_minutos_bloqueio_ate_ocioso()
        if minutos > 0 and not PREVIEW:
            self._timer_ocioso_id = GLib.timeout_add_seconds(
                minutos * 60, self._entrar_modo_ocioso
            )
        elif PREVIEW_OCIOSO:
            # Depois do show_all() do preview, senao o hide() do rodape
            # nao pega. Ver PREVIEW_OCIOSO no topo do arquivo.
            GLib.idle_add(self._entrar_modo_ocioso)
        self.add_events(
            Gdk.EventMask.KEY_PRESS_MASK
            | Gdk.EventMask.BUTTON_PRESS_MASK
            | Gdk.EventMask.POINTER_MOTION_MASK
        )
        self.connect("key-press-event", self._atividade)
        self.connect("button-press-event", self._atividade)
        self.connect("motion-notify-event", self._atividade)

    def _avatar(self):
        """Foto do usuario, recortada em circulo.

        Procura ~/.face, que e a convencao usada por praticamente todos
        os gerenciadores de sessao do Linux (GDM, SDDM, LightDM) - assim
        a mesma foto serve pra ca e pra qualquer um deles depois.
        Sem o arquivo, usa o icone generico do tema.
        """
        caminho = os.path.join(GLib.get_home_dir(), ".face")
        if not os.path.isfile(caminho):
            caminho = os.path.join(GLib.get_home_dir(), ".face.icon")

        imagem = Gtk.Image()
        imagem.set_name("avatar")
        # Sem isto o widget ocupa a largura toda do container e a borda
        # arredondada do CSS vira uma elipse atravessando a tela - a
        # borda segue a AREA do widget, nao o tamanho da imagem.
        imagem.set_halign(Gtk.Align.CENTER)
        imagem.set_size_request(TAM_AVATAR, TAM_AVATAR)
        if os.path.isfile(caminho):
            try:
                pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(
                    caminho, TAM_AVATAR, TAM_AVATAR, False
                )
                imagem.set_from_surface(self._circular(pixbuf))
            except GLib.Error:
                imagem.set_from_icon_name("avatar-default", Gtk.IconSize.DIALOG)
                imagem.set_pixel_size(TAM_AVATAR)
        else:
            imagem.set_from_icon_name("avatar-default", Gtk.IconSize.DIALOG)
            imagem.set_pixel_size(TAM_AVATAR)
        return imagem

    def _circular(self, pixbuf):
        """Recorta o pixbuf num circulo.

        O GTK3 nao tem borda circular pra imagem: "border-radius" no CSS
        so afeta o fundo do widget, nao o conteudo. Por isso o recorte e
        feito na mao com cairo.
        """
        import cairo

        lado = TAM_AVATAR
        superficie = cairo.ImageSurface(cairo.FORMAT_ARGB32, lado, lado)
        ctx = cairo.Context(superficie)
        ctx.arc(lado / 2, lado / 2, lado / 2, 0, 2 * 3.141592653589793)
        ctx.clip()
        Gdk.cairo_set_source_pixbuf(ctx, pixbuf, 0, 0)
        ctx.paint()
        return superficie

    def _montar_video(self, categoria_dir, escolha=None):
        tipo, caminho = escolha if escolha is not None else escolher_midia(categoria_dir)
        if not caminho:
            return None
        if tipo == "foto":
            # Foto nao precisa de GStreamer nenhum - so um Gtk.Image que
            # se estica pra tela toda.
            imagem = Gtk.Image()

            def encaixar(widget, alocacao):
                try:
                    pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(
                        caminho, alocacao.width, alocacao.height, False
                    )
                    widget.set_from_pixbuf(pixbuf)
                except GLib.Error:
                    pass

            imagem.connect("size-allocate", encaixar)
            return imagem
        Gst.init(None)
        # O sink entrega um widget GTK pronto; sem ele o video abriria numa
        # janela propria, fora da nossa.
        #
        # PREFERIMOS gtkglsink (dentro de um glsinkbin) e NAO o gtksink.
        # Os dois desenham num widget, mas o gtksink exige o quadro em
        # memoria de SISTEMA: com o decode acontecendo na GPU (vaav1dec /
        # vavp9dec / vah264dec), cada quadro tem que ser baixado da GPU e
        # convertido de cor na CPU, 60x por segundo. Medido em 10/08/2026
        # com a tela bloqueada de verdade: video_lock.py em 133% de CPU -
        # 199 minutos de CPU em 2h de tela trancada, mais que o desktop
        # inteiro somado. O gtkglsink mantem o quadro na GPU.
        #
        # O gtksink continua como plano B: se o GL nao negociar (driver,
        # container sem /dev/dri), melhor tela de bloqueio pesada do que
        # tela de bloqueio preta.
        widget_sink = Gst.ElementFactory.make("gtkglsink")
        if widget_sink is not None:
            sink = Gst.ElementFactory.make("glsinkbin")
            if sink is not None:
                sink.set_property("sink", widget_sink)
            else:
                widget_sink = None
        if widget_sink is None:
            print("gtkglsink/glsinkbin indisponivel - caindo pro gtksink", file=sys.stderr)
            sink = Gst.ElementFactory.make("gtksink")
            widget_sink = sink
        if sink is None:
            return None
        self.pipeline = Gst.ElementFactory.make("playbin")
        if self.pipeline is None:
            return None
        self.pipeline.set_property("video-sink", sink)
        self.pipeline.set_property("uri", GLib.filename_to_uri(caminho, None))
        # Papel de parede nao dispute o som do sistema.
        self.pipeline.set_property("mute", True)

        barramento = self.pipeline.get_bus()
        barramento.add_signal_watch()
        barramento.connect("message::eos", self._reiniciar)
        barramento.connect("message::error", self._erro_gst)

        # O widget vem do gtkglsink/gtksink, nunca do glsinkbin (que e so
        # o involucro GL e nao tem essa propriedade).
        widget = widget_sink.get_property("widget")
        # NAO damos PLAY aqui: o sink so tem superficie depois que o
        # widget e realizado na tela. Comecando antes, o pipeline roda mas
        # nao ha onde desenhar - o fundo fica preto, e sem erro nenhum,
        # que foi exatamente o sintoma.
        widget.connect("realize", lambda *_: self.pipeline.set_state(Gst.State.PLAYING))
        return widget

    def _trocar_fundo(self, categoria_dir):
        """Para o fundo atual e coloca midia da categoria (bloqueio/ocioso)."""
        escolha = escolher_midia(categoria_dir)
        tipo, caminho = escolha
        if not caminho:
            return

        # VIDEO -> VIDEO NAO RECONSTROI WIDGET NENHUM, so troca a URI do
        # playbin. Destruir o widget do gtkglsink e montar outro no lugar
        # e o caminho mais curto pra derrubar a tela de bloqueio inteira:
        #
        #   gstreamer: gst-resource-error-quark: Failed to initialize
        #              OpenGL with Gtk (3)
        #   Gdk-WARNING **: eglMakeCurrent failed
        #   ... core dump
        #
        # O contexto GL do widget velho ainda nao foi solto quando o novo
        # pede o dele. E como isso acontece justamente na virada pro modo
        # ocioso (10 min depois de bloquear), a sessao ficava trancada
        # sem cliente de bloqueio - a tela vermelha do sway. Reproduzido
        # com `--preview --preview-ocioso`.
        if tipo == "video" and self.pipeline is not None:
            self.pipeline.set_state(Gst.State.READY)
            self.pipeline.set_property("uri", GLib.filename_to_uri(caminho, None))
            self.pipeline.set_state(Gst.State.PLAYING)
            return

        # Sobra a troca de TIPO (foto <-> video), que exige widget novo.
        # Aqui nao ha dois sinks GL disputando: ou o antigo era foto (sem
        # GL), ou o novo e foto (idem).
        self.parar_video()
        if self._widget_fundo is not None and self.pilha is not None:
            self.pilha.remove(self._widget_fundo)
            self._widget_fundo.destroy()
            self._widget_fundo = None
        self.pipeline = None
        novo = self._montar_video(categoria_dir, escolha)
        if novo is None:
            novo = Gtk.Box()
        self._widget_fundo = novo
        # Gtk.Overlay: add() define o filho principal (fundo); overlays
        # (veu, formulario) ja estao em add_overlay e permanecem.
        self.pilha.add(novo)
        novo.show_all()
        # ...permanecem no arvore, mas iam parar ATRAS do video. O widget
        # do gtkglsink tem GdkWindow propria, e no GTK3 quem e realizado
        # depois fica por cima dentro do mesmo pai - o fundo novo nasce
        # depois do veu. Sintoma: ao entrar no modo ocioso o relogio e a
        # data sumiam (nao "mudavam de lugar"), e voltavam sozinhos ao
        # sair. Reancorar o veu recria a janela dele no topo da pilha.
        # Nada de show_all() aqui: no modo ocioso o rodape esta escondido
        # de proposito e show_all() o traria de volta.
        self.pilha.remove(self.veu)
        self.pilha.add_overlay(self.veu)
        if self.pipeline is not None and novo.get_realized():
            self.pipeline.set_state(Gst.State.PLAYING)

    def _entrar_modo_ocioso(self):
        """Esconde formulario e toca midia de ocioso; sessao continua trancada."""
        if self.modo_ocioso:
            return False
        self.modo_ocioso = True
        self.rodape.hide()
        # Sem o rodape, o espaco dele volta pro bloco do relogio. Alinhado
        # ao FIM, o bloco afundava pro pe da tela (a data ficava na ultima
        # linha de pixels). Centralizado, relogio+data ficam no meio - o
        # lugar certo numa tela sem formulario.
        self.bloco_relogio.set_valign(Gtk.Align.CENTER)
        self.aviso.set_text("")
        # Pasta ocioso vazia: o _trocar_fundo sai sem mexer no fundo atual.
        self._trocar_fundo(DIR_OCIOSO)
        self._timer_ocioso_id = None
        return False  # nao repetir o timeout

    def _sair_modo_ocioso(self):
        """Volta o formulario de senha (ainda bloqueado)."""
        if not self.modo_ocioso:
            return
        self.modo_ocioso = False
        self.rodape.show_all()
        # Volta o bloco do relogio pro lugar de sempre (ver _entrar_modo_ocioso).
        self.bloco_relogio.set_valign(Gtk.Align.END)
        self.senha.grab_focus()
        # Volta midia de bloqueio (convencao da tela de login/lock).
        self._trocar_fundo(DIR_BLOQUEIO)
        minutos = ler_minutos_bloqueio_ate_ocioso()
        if minutos > 0:
            if self._timer_ocioso_id is not None:
                GLib.source_remove(self._timer_ocioso_id)
            self._timer_ocioso_id = GLib.timeout_add_seconds(
                minutos * 60, self._entrar_modo_ocioso
            )

    def _barra_energia(self):
        """Suspender, reiniciar e desligar. O polkit libera os tres sem senha
        para a sessao local ativa - conferido com pkcheck."""
        caixa = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=0)
        caixa.set_name("energia")
        caixa.set_halign(Gtk.Align.END)
        caixa.set_valign(Gtk.Align.START)
        caixa.set_margin_top(12)
        caixa.set_margin_end(12)
        for icone, dica, cmd in (
            ("system-suspend-symbolic", "Suspender", "systemctl suspend"),
            ("system-reboot-symbolic", "Reiniciar", "systemctl reboot"),
            ("system-shutdown-symbolic", "Desligar", "systemctl poweroff"),
        ):
            b = Gtk.Button()
            b.set_image(Gtk.Image.new_from_icon_name(icone, Gtk.IconSize.LARGE_TOOLBAR))
            b.set_tooltip_text(dica)
            b.set_relief(Gtk.ReliefStyle.NONE)
            b.connect("clicked", self._energia, cmd, dica)
            caixa.pack_start(b, False, False, 0)
        return caixa

    def _energia(self, _botao, comando: str, dica: str) -> None:
        if PREVIEW:
            # Sem isto, testar o visual da tela desligaria a maquina.
            self.aviso.set_text(f"(pre-visualizacao: {dica} nao executado)")
            return
        try:
            subprocess.Popen(comando.split())
        except OSError as e:
            self.aviso.set_text(f"falhou: {e}")

    def alternar_teclado(self):
        """Mostra ou esconde o teclado. Chamado pelo SIGUSR1 e pelo botao."""
        if self.teclado is None:
            self.teclado = TecladoEmbutido(load_config(), debug_alpha=DEBUG_ALPHA)
            if not self.teclado.parse_arguments(["deck-lock"]):
                self.teclado = None
                self.aviso.set_text("nao consegui montar o teclado")
                return
            # Sem controle, o teclado precisa estar visivel e clicavel; com
            # controle, e o modo fantasma de sempre.
            self.modo_mouse = FORCA_MOUSE or not TecladoEmbutido.ha_controle()
            widget = self.teclado.montar(
                ao_teclar=self._tecla_clicada if self.modo_mouse else None,
            )
            # X, Y, C e STEAM+B chamam OSK.close(), que chega no quit() do
            # teclado; aqui isso significa esconder, nao encerrar a tela.
            self.teclado.definir_ao_fechar(self._esconder_teclado)
            # O caps travado pela tecla desenhada nao passa pelo Caps Lock do
            # sistema, entao o keymap nao avisa ninguem: quem avisa e o teclado.
            self.teclado.definir_ao_modificar(self._sync_capslock)
            self.caixa_teclado.add(widget)
            # Sem controle nao ha o que travar nem pads que ler: conectar ao
            # daemon so renderia um erro de lock e um teclado inerte.
            if not self.modo_mouse:
                self.teclado.ligar()
            carregar_css()  # o teclado registrou CSS proprio no meio do caminho
            self.caixa_teclado.show_all()
            self.teclado.cursores_fora()
            # So o modo mouse precisa de espaco: ele desenha o teclado inteiro.
            # No modo fantasma nada sai da frente, porque nada fica visivel.
            self._compactar(self.modo_mouse)
            return
        if self.caixa_teclado.get_visible():
            self._esconder_teclado()
        else:
            self.caixa_teclado.show_all()
            # Retoma os pads: o esconder os soltou, e sem religar o teclado
            # voltaria visivel e inerte.
            if not self.modo_mouse:
                self.teclado.ligar()
            self.teclado.cursores_fora()
            self._compactar(self.modo_mouse)

    def _sync_capslock(self, keymap=None) -> None:
        """Mostra o aviso pelo Caps Lock fisico OU pelo shift travado do teclado.

        Sao dois caps diferentes com o mesmo efeito para quem digita: o do
        teclado fisico e o duplo clique na tecla desenhada. Quem esta olhando a
        tela quer saber que sai maiuscula, nao de onde veio.
        """
        # getattr: o indicador e construido antes do atributo do teclado, e o
        # primeiro sync roda ainda dentro do __init__.
        teclado = getattr(self, "teclado", None)
        travado = teclado is not None and teclado.shift_travado
        self.capslock.set_visible(self.keymap.get_caps_lock_state() or travado)

    def _alternar_visibilidade(self) -> None:
        """Mostra ou oculta a senha digitada."""
        visivel = not self.senha.get_visibility()
        self.senha.set_visibility(visivel)
        self.senha.set_icon_from_icon_name(
            Gtk.EntryIconPosition.PRIMARY,
            "view-conceal-symbolic" if visivel else "view-reveal-symbolic",
        )
        self.senha.set_icon_tooltip_text(
            Gtk.EntryIconPosition.PRIMARY,
            "Ocultar a senha" if visivel else "Mostrar a senha",
        )

    def _tecla_clicada(self, nome: str) -> None:
        """Aplica no campo de senha a tecla clicada no modo mouse.

        Escreve direto no Gtk.Entry em vez de emitir por uinput: aqui o
        destino do texto e conhecido, e o caminho mais curto para a senha e
        tambem o mais simples de auditar.
        """
        if nome == "KEY_BACKSPACE":
            texto = self.senha.get_text()
            self.senha.set_text(texto[:-1])
            self.senha.set_position(-1)
            return
        if nome in ("KEY_ENTER", "KEY_KPENTER"):
            self._tentar(self.senha)
            return
        # Shift e AltGr agora sao teclas do proprio layout, e nao botoes ao
        # lado: clicar nelas alterna o nivel em vez de digitar. No modo
        # fantasma os grips fazem o mesmo, entao os dois modos combinam.
        if nome in TecladoEmbutido.MODIFICADORES:
            # Mesmo caminho do modo pad: um toque liga, dois travam (caps), e
            # com ele travado um toque desliga. A regra mora no teclado para
            # os dois modos nao divergirem.
            self.teclado.alternar_modificador(nome)
            return
        if nome == "KEY_SPACE":
            self.senha.set_text(self.senha.get_text() + " ")
            self.senha.set_position(-1)
            return
        # As demais vem do rotulo que o proprio teclado ja calcula a partir
        # do layout ativo - assim acentos e simbolos seguem o teclado do
        # sistema, sem tabela paralela aqui.
        botao = next(
            (b for b in self.teclado.background.buttons if b.name == nome), None
        )
        if botao is not None and botao.label and len(botao.label) == 1:
            # O rotulo ja vem do nivel ativo do layout: com shift ele e "!" e
            # nao "1", entao nao ha conversao a fazer aqui.
            self.senha.set_text(self.senha.get_text() + botao.label)
            self.senha.set_position(-1)
            # Shift simples vale uma tecla so; caps fica ate ser desligado.
            self.teclado.consumir_shift()

    def _esconder_teclado(self) -> None:
        """Esconde o teclado E devolve os pads.

        Esconder nao pode ser so um hide(): o teclado do lock trava LPAD/RPAD
        no daemon ao ligar(), e continua vivo depois de escondido. Sem soltar,
        os pads ficam presos a um teclado que ninguem ve — e o pad para de
        mover o cursor, que e justamente por que se esconde o teclado.

        No desktop isso nunca apareceu porque la fechar o teclado encerra o
        processo, e os locks morrem junto.
        """
        if self.caixa_teclado.get_visible():
            self.caixa_teclado.hide()
            if not self.modo_mouse:
                self.teclado.desligar()
            else:
                # Sem daemon nao ha desligar(), mas o caps travado tem de cair
                # junto: com o teclado fora da tela nao ha como desfaze-lo.
                self.teclado.soltar_modificadores()
            self._compactar(False)

    def _compactar(self, ligado: bool) -> None:
        """Abre espaco para o teclado escondendo o que nao e essencial.

        O teclado ocupa 405px de altura e a tela do Deck tem 500 logicos:
        com relogio, avatar e nome no lugar, o campo de senha some atras
        dele. Digitando, o que precisa estar visivel e o campo.
        """
        for w in (self.bloco_relogio, self.avatar, self.rotulo_nome):
            w.hide() if ligado else w.show()
        # O veu ocupa a tela toda e o rodape fica centralizado nele: sem
        # encolher o veu, o campo de senha cai bem atras do teclado. A margem
        # reserva a faixa de baixo, e o campo sobe para a area que sobra.
        altura = self.caixa_teclado.get_allocated_height() if ligado else 0
        if ligado and altura < 50:      # ainda nao alocado no primeiro toggle
            altura = 405                # tamanho do SVG do teclado
        self.veu.set_margin_bottom(altura)
        # O rodape tem 60+80px de margem para respirar na tela cheia; na
        # faixa que sobra acima do teclado isso nao cabe e corta o campo.
        self.rodape.set_margin_top(0 if ligado else 60)
        self.rodape.set_margin_bottom(0 if ligado else 80)

    def _atividade(self, _w, _evento):
        if self.modo_ocioso:
            self._sair_modo_ocioso()
            return True
        # Reinicia o timer de ocioso enquanto digita/mexe no formulario.
        minutos = ler_minutos_bloqueio_ate_ocioso()
        if minutos > 0 and not PREVIEW:
            if self._timer_ocioso_id is not None:
                GLib.source_remove(self._timer_ocioso_id)
            self._timer_ocioso_id = GLib.timeout_add_seconds(
                minutos * 60, self._entrar_modo_ocioso
            )
        return False

    def _erro_gst(self, _bus, msg):
        erro, _debug = msg.parse_error()
        print(f"gstreamer: {erro}", file=sys.stderr)

    def _reiniciar(self, *_):
        # Sem isto o video congela no ultimo quadro ao terminar.
        self.pipeline.seek_simple(
            Gst.Format.TIME, Gst.SeekFlags.FLUSH | Gst.SeekFlags.KEY_UNIT, 0
        )

    def _tique(self):
        agora = GLib.DateTime.new_now_local()
        self.relogio.set_text(agora.format("%H:%M"))
        self.data.set_text(agora.format("%A, %d de %B").capitalize())
        return GLib.SOURCE_CONTINUE

    def _tentar(self, _entry):
        senha = self.senha.get_text()
        self.senha.set_text("")
        # Diagnostico sem expor a senha: so o tamanho, se ha caractere fora do
        # ASCII e o encoding que o subprocess vai usar. Uma senha correta
        # recusada quase sempre e um destes tres - e o campo mostra o texto
        # certo de qualquer jeito, entao olhar a tela nao resolve.
        import locale

        journal_lock(
            f"_tentar: len={len(senha)} ascii={senha.isascii()} "
            f"encoding={locale.getpreferredencoding(False)} LANG={os.environ.get('LANG', '<vazio>')}"
        )
        if PREVIEW:
            self.aviso.set_text("(pre-visualizacao: senha nao e verificada)")
            return

        resultado = subprocess.run(
            [sys.executable, os.path.abspath(__file__), "--auth", USUARIO],
            input=senha + "\n",
            capture_output=True,
            text=True,
        )
        journal_lock(f"resultado: rc={resultado.returncode} err={resultado.stderr.strip()[:80]!r}")
        if resultado.returncode == 0:
            self.parar_video()
            self.ao_desbloquear()
        else:
            self.aviso.set_text("Senha incorreta")

    def parar_video(self):
        if self.pipeline is not None:
            self.pipeline.set_state(Gst.State.NULL)


# Todas as telas abertas (uma por monitor). O SIGUSR1 age na primeira: o
# controle e um so, entao o teclado tambem.
TELAS = []


def _alternar_pelo_sinal():
    """Handler do SIGUSR1, mandado pelo deck-osk --toggle."""
    if TELAS:
        TELAS[0].alternar_teclado()
    return GLib.SOURCE_CONTINUE


def carregar_css():
    # O placeholder do Gtk.Entry NAO usa a cor do nosso CSS: ele vem do tema
    # do sistema. Com tema claro sai cinza escuro - invisivel sobre o fundo
    # preto desta tela, e o campo parece uma caixa vazia sem rotulo nenhum.
    Gtk.Settings.get_default().set_property("gtk-application-prefer-dark-theme", True)

    css = CORES_PADRAO + CSS_BASE
    if os.path.isfile(CORES_CSS):
        with open(CORES_CSS, encoding="utf-8") as f:
            css = CORES_PADRAO + f.read() + CSS_BASE
    provedor = Gtk.CssProvider()
    try:
        provedor.load_from_data(css.encode())
    except GLib.Error as erro:
        print(f"CSS invalido: {erro}", file=sys.stderr)
        return
    # Acima de PRIORITY_USER (800): ao construir a janela do teclado, o
    # OSDWindow do sc-controller registra o CSS dele nessa prioridade, no
    # escopo da tela. Em APPLICATION (600) o nosso perderia.
    Gtk.StyleContext.add_provider_for_screen(
        Gdk.Screen.get_default(), provedor, Gtk.STYLE_PROVIDER_PRIORITY_USER + 200
    )


def _marcar_ativo():
    """Registra o pid e apaga na saida, aconteca o que acontecer."""
    import atexit

    try:
        os.makedirs(os.path.dirname(PID_LOCK), exist_ok=True)
        with open(PID_LOCK, "w") as f:
            f.write(str(os.getpid()))
    except OSError as e:
        print(f"aviso: nao consegui gravar {PID_LOCK} ({e})", file=sys.stderr)
        return

    def limpar():
        try:
            os.unlink(PID_LOCK)
        except OSError:
            pass

    atexit.register(limpar)
    for s in (signal.SIGTERM, signal.SIGINT):
        anterior = signal.getsignal(s)
        signal.signal(
            s, lambda sig, frm, a=anterior: (limpar(), a(sig, frm) if callable(a) else sys.exit(0)),
        )


def main():
    carregar_css()
    _marcar_ativo()

    # Com o teclado de desktop aberto, ele segura o controle: o primeiro
    # STEAM+B ia para ele (fechando-o) em vez de chegar aqui, e so o segundo
    # abria o teclado embutido. Alem disso ele ficaria invisivel atras desta
    # tela. Encerrar de saida resolve os dois.
    try:
        from deck_osk import running_pid

        pid = running_pid()
        if pid is not None:
            os.kill(pid, signal.SIGTERM)
    except (OSError, ImportError):
        pass
    # GLib.unix_signal_add esta depreciado em favor de GLibUnix.signal_add,
    # que nao existe em versoes mais antigas do PyGObject.
    try:
        gi.require_version("GLibUnix", "2.0")
        from gi.repository import GLibUnix

        GLibUnix.signal_add(GLib.PRIORITY_DEFAULT, signal.SIGUSR1, _alternar_pelo_sinal)
    except (ImportError, ValueError):
        GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGUSR1, _alternar_pelo_sinal)

    if PREVIEW:
        # Janela layer-shell comum: cobre a tela e mostra o mesmo visual,
        # mas o compositor NAO esta bloqueado - da pra sair matando o
        # processo, ou pelo Esc abaixo.
        janela = TelaBloqueio(Gtk.main_quit)
        TELAS.append(janela)
        GtkLayerShell.init_for_window(janela)
        GtkLayerShell.set_layer(janela, GtkLayerShell.Layer.OVERLAY)
        for borda in (GtkLayerShell.Edge.LEFT, GtkLayerShell.Edge.RIGHT,
                      GtkLayerShell.Edge.TOP, GtkLayerShell.Edge.BOTTOM):
            GtkLayerShell.set_anchor(janela, borda, True)
        GtkLayerShell.set_keyboard_mode(
            janela, GtkLayerShell.KeyboardMode.EXCLUSIVE
        )
        janela.connect(
            "key-press-event",
            lambda _w, e: Gtk.main_quit() if e.keyval == Gdk.KEY_Escape else None,
        )
        janela.show_all()
        janela.senha.grab_focus()
        Gtk.main()
        return

    gi.require_version("GtkSessionLock", "0.1")
    from gi.repository import GtkSessionLock

    # A API do gtk-session-lock NAO tem sinais - confirmado com
    # GObject.signal_list_names(), que devolve tupla vazia, e no
    # cabecalho C (gtk-session-lock.h). A primeira versao deste arquivo
    # conectava "locked" e "failed" e quebrava com
    # "unknown signal name: failed"; pior, mesmo sem o erro as surfaces
    # nunca teriam sido criadas, porque eram criadas dentro do handler.
    # O fluxo correto e direto: prepare -> lock -> new_surface por
    # monitor.
    if not GtkSessionLock.is_supported():
        print("compositor nao suporta ext-session-lock", file=sys.stderr)
        sys.exit(1)

    trava = GtkSessionLock.prepare_lock()
    trava.lock_lock()

    # UMA SURFACE POR MONITOR, E O CONJUNTO MUDA EM TEMPO DE EXECUCAO.
    #
    # Criar as surfaces so uma vez, no inicio, foi o bug que deixou a
    # maquina inutilizavel na noite de 09/08/2026: o monitor (LG por HDMI)
    # se desconectou e reconectou sozinho durante a madrugada - da pra ver
    # no log do container, "Bar removed from output: HDMI-A-1" seguido de
    # "Bar configured" um segundo depois. O output novo nasceu SEM surface
    # de bloqueio, e o sway, com a sessao trancada e nada pra desenhar,
    # pinta a saida inteira de VERMELHO. Pior: o processo ainda morria
    # junto, e ai a trava virava orfa - matar o que sobrou nao destravava
    # nada, porque quem manda no estado e o compositor.
    #
    # Por isso o ciclo de vida das surfaces acompanha os sinais do
    # GdkDisplay. O unmap antes de destruir a janela e exigencia da
    # propria lib (gtk-session-lock.h: "must be called before the window
    # is unmapped"); sem ele o cliente segue mexendo numa surface que o
    # compositor ja invalidou, o que derruba o processo por erro de
    # protocolo.
    janelas = {}

    def destravar():
        for j in list(janelas.values()):
            j.parar_video()
        trava.unlock_and_destroy()
        # Sem o sync o compositor pode ainda nao ter processado o
        # desbloqueio quando o processo morre, e a sessao fica travada.
        Gdk.Display.get_default().sync()
        Gtk.main_quit()

    def cobrir(monitor):
        """Poe uma tela de bloqueio no monitor (novo ou ja existente)."""
        if monitor in janelas:
            return
        janela = TelaBloqueio(destravar)
        janelas[monitor] = janela
        TELAS.append(janela)
        # "You must only ever call this method once for a given lock and
        # monitor" - o monitor que volta depois de um reconecte e outro
        # objeto GdkMonitor, entao isto e legitimo.
        trava.new_surface(janela, monitor)
        janela.show_all()
        janela.senha.grab_focus()

    def descobrir(monitor):
        """Monitor sumiu: solta a surface dele sem derrubar o processo."""
        janela = janelas.pop(monitor, None)
        if janela is None:
            return
        if janela in TELAS:
            TELAS.remove(janela)
        janela.parar_video()
        GtkSessionLock.unmap_lock_window(janela)
        janela.destroy()

    tela = Gdk.Display.get_default()
    tela.connect("monitor-added", lambda _d, monitor: cobrir(monitor))
    tela.connect("monitor-removed", lambda _d, monitor: descobrir(monitor))
    for i in range(tela.get_n_monitors()):
        cobrir(tela.get_monitor(i))

    Gtk.main()


if __name__ == "__main__":
    # Qualquer excecao aqui tem que virar SAIDA DIFERENTE DE ZERO e texto
    # no log: e por esse codigo que o lock_screen.sh sabe distinguir
    # "desbloqueou" de "caiu com a sessao trancada" - e so no segundo caso
    # ele sobe a tela de novo, em vez de deixar a saida vermelha do sway.
    try:
        main()
    except Exception:
        import traceback

        traceback.print_exc()
        sys.stderr.flush()
        sys.exit(1)
