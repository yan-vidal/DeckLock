# Usando o DeckLock

O DeckLock é um bloqueador de tela experimental e personalizável para Wayland. Inclui teclado virtual, temas externos e biblioteca de mídias. A versão 0.2 acrescenta fundos procedurais e um guia offline.

## Comece pelo preview

Abra as configurações pelo menu de aplicativos ou execute:

```sh
decklock --settings
```

Abra o preview para experimentar mudanças. Ele não bloqueia a sessão, não autentica senhas e não executa ações de energia. Escape fecha o preview da tela de bloqueio. Fechá-lo nunca desbloqueia uma sessão real.

Use F1 ou o botão de interrogação para abrir este guia. Geral explica o uso diário; Avançada apresenta a implementação. O guia acompanha o idioma selecionado nas configurações e funciona sem internet.

## Salvar e controles das janelas

As configurações mostram as mudanças no preview antes de salvá-las. Salvar grava a configuração. Fechar as configurações descarta alterações não salvas e fecha seu preview.

Todas as janelas comuns do DeckLock oferecem um botão de fechar por padrão. Em Layout, desative as barras de título se preferir os atalhos do compositor. A ajuda continua disponível por F1. Essa preferência não acrescenta controles à tela realmente bloqueada.

O editor de procedurais mostra mudanças válidas imediatamente. Aplicar aceita o rascunho nas configurações; Salvar o grava. Fechar esse editor sem Aplicar restaura os parâmetros anteriores. O editor de CSS/layout mantém seu rascunho para o botão Salvar principal; fechar apenas o editor não grava no disco.

## Biblioteca e pool selecionado

A biblioteca contém imagens, vídeos e procedurais disponíveis. O pool à direita contém os itens elegíveis para o próximo bloqueio. Use a seta para adicionar a mídia selecionada; removê-la do pool não apaga o arquivo original. Importar copia seus arquivos para a biblioteca do usuário.

O olho abre um visualizador único e reutilizável. Vídeos são reproduzidos sem áudio. Procedurais têm uma engrenagem na biblioteca e no visualizador. Suas miniaturas acompanham as edições.

Cada bloqueio escolhe um item inicial do pool. Se for imagem, somente as imagens do pool participam da apresentação no intervalo configurado. Um vídeo escolhido fica em repetição naquele bloqueio. Um procedural escolhido continua animando naquele bloqueio. Um pool explicitamente vazio mostra o fundo alternativo.

## Procedurais e medições

Os efeitos disponíveis são Campo de estrelas, Partículas, curvas de Lissajous, chuva Matrix, fogo Doom, Aurora, Campo de fluxo e Cordilheira. A velocidade começa em 1; densidade, cores, semente e limite de quadros dependem do efeito. O procedural substitui o fundo.

O visualizador informa FPS gerados, CPU de desenho, CPU total do processo de configurações e memória do processo. Os totais incluem GTK e outras tarefas das configurações; não representam o custo exclusivo do efeito. O tamanho da textura estima o buffer de pixels, não toda a memória da GPU. Desempenho e bateria no dispositivo podem diferir dessas medições.

## Repouso

Repouso é o modo de inatividade do próprio DeckLock depois de iniciado. Ele não agenda a abertura do bloqueador pelo sistema. Configure seu ambiente ou daemon de inatividade separadamente para iniciar o DeckLock.

A aba Repouso mostra uma prévia desse estado. Escolha um pool e intervalo próprios, mantenha o fundo normal ocultando os controles ou desative o repouso. O relógio em repouso é configurável. A atividade restaura a interface normal.

## Tentativas e bloqueio da conta

Sistemas Linux costumam contar falhas de autenticação e bloquear a conta por um tempo. Isso é política do PAM, não do DeckLock, e vale igualmente para o login no terminal.

Quando o PAM informa um bloqueio da conta, a tela mostra o aviso e, quando disponível, um contador estimado. A estimativa vem dos minutos arredondados pelo PAM; não promete quando a autenticação será aceita. O campo de senha continua utilizável durante a contagem. Avisos de tentativas restantes aparecem apenas se o próprio PAM os fornecer; o DeckLock não calcula a política do sistema.

Nada disso é imposto pelo DeckLock: ele só repete o que o sistema informou. Se faltar informação, a tela fica em silêncio em vez de estimar um número. Os avisos usam os seletores `#status.warning` e `#status.locked`, que o seu tema pode estilizar.

## Teclado e energia

Use o teclado físico ou virtual. Shift altera a caixa das letras, dois toques em Shift mantêm Caps Lock e Alt mostra caracteres alternativos. O olho ao lado da senha alterna sua visibilidade. A integração opcional com sc-controller usa seu daemon externo e o teclado embutido.

Suspender, hibernar, reiniciar e desligar são pedidos ao systemctl. Dependem do suporte, permissões e configuração do sistema; o DeckLock não configura a hibernação. Essas ações ficam desativadas no preview.

## Temas, mídias e terminal

Escolha uma paleta incluída ou um tema externo. O tema combina CSS do GTK e layout declarativo em TOML. As edições são validadas antes de salvar. Restaurar devolve os controles correspondentes ou o rascunho do tema aos padrões incluídos.

O vídeo de Osaka incluído foi fornecido pelo autor. Mídias importadas ficam em XDG_DATA_HOME/decklock/library, normalmente ~/.local/share/decklock/library. As pastas legadas de bloqueio e repouso ficam em XDG_CONFIG_HOME/midias/bloqueio e midias/ocioso, com subpastas fotos e videos. Configurações explícitas e pools têm prioridade sobre os padrões.

```sh
decklock --help
decklock config path
decklock config show
decklock config set window_decorations false
decklock config set procedurals.matrix.speed 1.0
decklock config unset window_decorations
```

O bloqueio real exige um compositor com suporte ao protocolo de bloqueio de sessão do Wayland. Teste seu procedimento de recuperação antes de depender deste bloqueador experimental no uso diário.
