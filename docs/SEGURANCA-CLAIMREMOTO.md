# Segurança do ClaimRemote — modelo de confiança e quem decide o pagamento

Responde: "o relayer não pode decidir quem recebe; isso tem de estar no contrato".

## 1. Dois "beneficiários" diferentes — não confundir

| Termo | Quem é | Papel |
|---|---|---|
| **`beneficiary` do IGP** | **o contrato VAULT** | recebe a arrecadação de gás da ponte (é para onde o `claim()` do IGP empurra os fundos). É dinheiro ENTRANDO no pool. |
| **`executor` (ClaimRemote)** | um **operador** (endereço vinculado) | recebe a recompensa da taxa de origem por uma entrega remota. É dinheiro SAINDO do pool. |

O vault é o **cofre**: o gás entra nele (como IGP beneficiary) e a recompensa
sai dele para o operador (como pagamento de ClaimRemote). O relayer nunca toca
no cofre — só **aciona** funções cujas regras estão 100% no contrato.

## 2. O que o CONTRATO decide (não o relayer)

O relayer/agente só EXECUTA a transação. Cada regra abaixo é imposta pelo
bytecode — modificar o agente não as contorna:

1. **Destinatário travado por allowlist.** O pagamento só vai para um endereço
   que o **owner/governança** vinculou (`remoteBinding`). Um agente malicioso
   NÃO consegue redirecionar para uma carteira de atacante nova — a tx reverte
   com `NoBinding`. Quem decide o conjunto de destinatários é o owner, no contrato.
2. **1 pagamento por `message_id`** (`remote_claimed`, effects-first). Nunca paga
   a mesma mensagem 2×, mesmo sob reentrância (guard) ou corrida entre agentes.
3. **Teto fixo por domínio** (`remote_reward`). Um id forjado custa no máximo a
   recompensa do domínio, nunca o pool inteiro. Só o owner muda o teto.
4. **Anti-autopagamento (quórum ≥ 2).** A atestação do PRÓPRIO beneficiário não
   conta para o quórum: pagar o operador X exige `quorum` atestadores
   INDEPENDENTES (≠ X). Uma chave sozinha não se paga. (EVM/CW: exclusão do voto
   próprio; Solana: `quorum` relatórios byte-idênticos de operadores distintos.)
5. **Pausa de emergência** (`SetPause`) congela toda atestação.

## 3. O limite físico (e o que ele exige)

A chain de ORIGEM **não enxerga** a chain de destino. Logo, "a mensagem X foi
entregue lá pelo operador Z" é a ÚNICA afirmação que o contrato não pode
verificar sozinho — ela vem dos atestadores. Isso é irredutível sem um dos dois:

- **(hoje) Atestação com quórum** — confiança distribuída entre N operadores
  independentes. Seguro se **quórum ≥ 2** e os operadores forem separados
  (chaves/máquinas distintas). **Com quórum = 1 (fase de teste, 1 operador) a
  chave única é a autoridade — nenhum contrato remove isso; é a definição de 1
  operador.** Por isso quórum = 1 é EXPLICITAMENTE só-teste.
- **(alvo trustless) Recibo de volta via Hyperlane** — o vault de DESTINO (que
  PODE verificar a entrega on-chain, via `processor(id)`/DELIVERIES) despacha uma
  mensagem de volta para o vault de ORIGEM afirmando "id X entregue por Z". Essa
  mensagem passa pela MESMA segurança de validadores/ISM da ponte. O vault de
  origem a recebe pelo seu Mailbox e paga — **sem confiar em atestador nenhum**;
  o contrato determina o destinatário a partir de uma mensagem assinada pelos
  validadores. Custo: o gás de uma mensagem de retorno por entrega. É o caminho
  para eliminar 100% a confiança — está proposto, não implementado (decisão da
  governança pelo custo/benefício).

## 4. Resposta direta à preocupação

> "Código malicioso no relayer é um problema."

Verdadeiro, e mitigado em camadas: um agente comprometido **detém a chave de UM
operador** — ou seja, equivale a UM atestador malicioso. Contra isso:
- ele **não** pode pagar endereço fora da allowlist (regra 1);
- ele **não** pode pagar 2× nem acima do teto (regras 2–3);
- com **quórum ≥ 2** ele **não** atinge o quórum sozinho (regra 4) — precisa
  comprometer N operadores independentes ao mesmo tempo.

O único cenário em que uma chave sozinha tem autoridade total é **quórum = 1**,
que existe apenas porque hoje há **1 operador em teste**. A ação de segurança
para produção é **operacional e já suportada pelo contrato**: adicionar
operadores independentes e subir o quórum (`MANUAL-EXPANSAO.md` §3). O
anti-autopagamento (regra 4) já está no bytecode, pronto para o momento em que o
quórum passar de 1.

## 5. Estado do código

- Regra 4 implementada e testada: EVM 39 testes · CW 30 testes (`cargo`+`forge`
  verdes). CW rebuild reproduzível: sha256
  `ee8893da963bb2dd6eb20a6090f241e80c523f867a5b2a923baa5f601cce29d4`.
- **Deploy:** a regra 4 é NO-OP em quórum = 1 (idempotente com o que já está no
  ar), então não exige redeploy para a fase de teste. Deve ser implantada
  **antes** de qualquer mudança para quórum ≥ 2 (bundle com o go-live
  multi-operador): TC `migrate` + EVM redeploy + config.
