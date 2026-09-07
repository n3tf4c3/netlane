# Motor WFP — política nativa de conexão

## Estratégia implementada em 2026-09-06

A API [FwpmConnectionPolicyAdd0](https://learn.microsoft.com/en-us/windows/win32/api/fwpmu/nf-fwpmu-fwpmconnectionpolicyadd0) permite roteamento por executável no próprio Windows. O motor usa AppId do caminho completo e LUID da interface escolhida. Não necessita de callout ou driver próprio neste Windows compatível.

- Sessão dinâmica com limpeza pelo BFE ao encerrar.
- Políticas IPv4/IPv6 em transação por executável; atualização com rollback.
- Condições de AppId e tráfego unicast não-loopback.
- LUID marshaled como ponteiro UINT64, com testes de layout nativo de 64 bits.
- BLOCK apenas para o modo de bloqueio explícito; não há bloqueio de outras interfaces para emular direcionamento.
- Sem API/privilégio/routepolicies, o serviço informa não aplicado.
- JSON preservado; heartbeat com revisão, estado e resultado por executável.
- Steam pode incluir o auxiliar conhecido steamwebhelper.exe, com opção de desativar e precedência de regra explícita.

A [documentação operacional](roteamento-nativo.md) contém os pré-requisitos, comandos de teste e reversão.

## Evidências e pendências

O build e 95 testes passaram. A prova TCP/UDP sem política funciona. A sessão atual não é administrativa e as opções routepolicies estão desativadas, então a prova elevada foi recusada antes de instalar políticas.

Ainda é necessário verificar:

1. --verify-routing elevado: novas conexões IPv4 TCP/UDP alternando Ethernet/Wi-Fi, e retorno automático após remoção.
2. Steam e steamwebhelper.exe reais após reinício normal, inclusive conexões IPv6.
3. Tráfego IPv6 em ambas as interfaces, aplicativos simultâneos, queda de interface, parada abrupta/BFE e interação com VPN/antivírus.
4. Tabela de rotas/métricas antes/depois e isolamento de aplicativos não selecionados.
5. Implantação do serviço e localização/ACL compartilhadas fora do checkout.

## Correção do diagnóstico anterior

O PERMIT/BLOCK anterior e o log filtros=2/2 não provavam seleção de saída. A consulta --check-public-ip continua sendo apenas uma referência da interface, pois usa curl com bind manual. A nova --verify-routing cria sockets sem bind/opções de interface no próprio executável alvo. Um sucesso nessa prova não certifica automaticamente o Steam.

O roteiro anterior de driver ALE bind/connect é uma alternativa de arquitetura para outros cenários, não um requisito do caminho nativo selecionado. Não foram instalados drivers nem desativadas proteções do Windows.
