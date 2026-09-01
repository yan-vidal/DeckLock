# Tela de bloqueio com teclado

Data: 2026-09-01

## Problema

O teclado fantasma não aparece na tela de bloqueio. Verificado por captura de tela feita
durante o lock, com o teclado forçado a opaco (`--debug-alpha`): aparece só o lockscreen.
As superfícies layer-shell continuam existindo — `hyprctl layers` lista todas — mas a
superfície de bloqueio é composta acima de todas. É a garantia do `ext-session-lock-v1`, para
que nenhum aplicativo desenhe sobre a tela de senha nem a espie.

As teclas, no entanto, **chegam**: o sc-controller emite por `uinput`, que para o lockscreen é
um teclado de kernel legítimo. Falta apenas o desenho.

## Ponto de partida

`video_lock.py` (658 linhas, Python/GTK3), do projeto `/opt/home-server/desktop` no servidor
de casa. É uma tela de bloqueio de verdade: usa `gtk-session-lock`, a **mesma biblioteca que o
gtklock** (`pacman -Qi gtklock` → depende de `gtk-session-lock`). O isolamento é idêntico ao
dele; quem garante que nada é desenhado por cima é o compositor, não o app.

Já resolve, e não será reescrito: relógio e data, fundo de foto ou vídeo sorteado, tema por
`cores.css`, avatar circular do `~/.face`, modo ocioso, uma surface por monitor com hotplug,
modo `--preview`, e PAM em subprocesso isolado.

Riscos herdados, assumidos conscientemente: superfície de ataque maior que a de um lock em C
(GTK + GStreamer + decode de vídeo) e muito menos auditoria. Em troca, é o único caminho que
entrega teclado na tela de bloqueio.

## Arquitetura

Uma janela não se embute em outra, e o teclado hoje é uma janela. Daí a extração:

    teclado.py    TecladoWidget: desenho Cairo e hit-test, sem janela
    deck-osk      hospeda o TecladoWidget numa janela layer-shell   (comportamento atual)
    deck-lock     hospeda o MESMO TecladoWidget dentro da tela de bloqueio

Assim o gradiente, o hit-test e a calibração já ajustados servem aos dois sem duplicação.

## Decisões

**Teclado sob demanda, não fixo.** `deck-osk --toggle` passa a checar o pidfile do lock antes
de agir: com lock ativo manda `SIGUSR1` para ele, que alterna o teclado embutido; sem lock,
mantém o comportamento de hoje. Um só atalho (STEAM+B), dois destinos. Some a isso um botão de
teclado ao lado do campo de senha, para quem está sem controle.

**Dois modos, escolhidos automaticamente** pela presença de controle no daemon do sc-controller:

| | Com controle | Sem controle |
|---|---|---|
| Aparência | fantasma, tela cheia | sólido, abaixo do campo de senha |
| Entrada | pads revelam ao toque | clique do mouse |
| Seleção | clique do pad, R2/L2 | clique |

O modo fantasma pressupõe pads: sem eles o teclado precisa estar visível para ser clicado. O
hit-test do modo mouse é o mesmo `button.contains(x, y)` que os cursores já usam.

**Digitação por uinput**, reusando o que já funciona. A alternativa seria inserir direto no
`Gtk.Entry`, um caminho mais curto para a senha; fica registrada caso se prefira depois.

**Ações de energia** (suspender, reiniciar, desligar) na própria tela. Verificado com `pkcheck`:
o polkit libera as três sem senha para a sessão local ativa.

## Etapas

1. Extrair `TecladoWidget` — sem mudança de comportamento; o `deck-osk` segue idêntico
2. `deck-lock` rodando com `--preview`, ainda sem teclado
3. Teclado no lock, modo controle
4. Modo mouse e detecção automática
5. Botões de energia

Só da etapa 2 em diante se mexe com bloqueio real, e sempre via `--preview` antes — a lição
mais cara que o `video_lock.py` já pagou: uma tela de bloqueio com defeito prende a sessão, e
a saída vira acesso externo.

## Dependências a instalar

`python-pam`, `gst-plugin-gtk`, `gst-plugins-bad`.

O código prefere `gtkglsink` a `gtksink` por um motivo medido no projeto original: com
`gtksink` cada quadro é baixado da GPU e convertido na CPU, o que rendeu 133% de CPU com a
tela bloqueada. Num aparelho a bateria isso pesa.

## Verificação

`--preview` abre a mesma tela como janela comum, sem bloquear o compositor. Critério de aceite
por etapa: (1) `deck-osk` continua funcionando igual; (2) o preview mostra relógio, fundo e
tema; (3) STEAM+B alterna o teclado dentro do preview; (4) sem controle, o teclado aparece
clicável e digita no campo; (5) os três botões de energia respondem.
