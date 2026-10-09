# Pads no login: validação física acompanhada

Esta etapa permanece pendente. Os testes isolados não comprovam acesso ao USB,
uinput, comportamento físico dos pads ou passagem para o daemon do desktop.
Use um ambiente recuperável e o pacote candidato correspondente ao commit do PR,
após os checks de pacote/VM passarem. Não use o executável local como pacote de release.

1. Anote versão/commit do candidato, versão do sc-controller, compositor, modelo
   do Deck e configuração de tela. Preserve o backup do greetd e uma TTY com acesso
   administrativo. A instalação e a saída da sessão devem ser acompanhadas pelo usuário.
2. Confira que a conta configurada em `default_session.user` consegue acessar o
   controlador e uinput pelas regras do pacote sc-controller. Não abra o HOME,
   não adicione permissões globais e não altere grupos como atalho.
3. Revise `sudo decklock setup all --compositor hyprland --output eDP-1 --transform 3 --controller --dry-run`
   para um OLED nesta orientação. Adapte saída/transformação para outro hardware.
   Aplique o mesmo comando sem `--dry-run` somente quando pronto para instalar.
   O setup não reinicia greetd; logout sozinho não ativa uma alteração do comando.
4. Salve o trabalho e encerre a sessão de forma acompanhada. Ative a configuração
   por reboot ou reinício de greetd a partir de uma TTY separada. Nunca reinicie
   greetd por um terminal da sessão que ele gerencia.
5. No login, use cada pad e seu gatilho/click para selecionar teclas. Verifique
   soltura independente, Shift/Alt pelos grips, Backspace/espaço pelos bumpers,
   ocultar o teclado, navegar pelo mouse do pad direito e reabrir pelo botão da UI.
   Confira também teclado físico e touchscreen.
6. Teste uma recusa de senha e uma autenticação correta digitando pessoalmente.
   Não grave senhas, OTPs ou texto do campo. Registre apenas rejeição/aceitação,
   a sessão escolhida e a ausência de outro teclado/OSD. Na sessão autenticada,
   confirme que o daemon do greeter terminou e que o sc-controller do desktop
   consegue assumir o controlador.
7. Faça novo login com o controle indisponível e confirme o teclado clicável.
   Teste depois o reinício do sc-controller durante um bloqueio recuperável,
   com o teclado aberto e fechado. A reconexão do locker é a funcionalidade da
   0.3.3; o daemon privado do greeter que morre não é reiniciado automaticamente.
8. Registre resultados por item e falhas de ergonomia/toque/GPU. Para desativar,
   repita as mesmas opções de setup com `--controller=false` e ative a alteração
   numa etapa acompanhada. Para recuperar o login, restaure o backup pela TTY.

Suspensão/hibernação, consumo de bateria e outros compositores continuam UATs
separados. Não execute ações de energia automaticamente como parte desta lista.
