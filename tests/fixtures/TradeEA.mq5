//+------------------------------------------------------------------+
//|                                                      TradeEA.mq5 |
//+------------------------------------------------------------------+
#include <Trade\Trade.mqh>
#include <ZedMqlStats.mqh>

input int InpMA_Period = 14;
input double InpLotSize = 0.1;

CTrade trade;
int bar_count = 0;

int OnInit() {
    ZedMqlInitEquity(ZEDMQL_SAMPLE_BAR);
    return(INIT_SUCCEEDED);
}

void OnTick() {
    ZedMqlSampleEquity();
    
    // Naive strategy: trade every few ticks just to generate deals
    bar_count++;
    
    if (bar_count % 100 == 0) {
        if (PositionsTotal() == 0) {
            if (bar_count % 200 == 0)
                trade.Buy(InpLotSize);
            else
                trade.Sell(InpLotSize);
        } else {
            trade.PositionClose(PositionGetSymbol(0));
        }
    }
}

double OnTester() {
    ZedMqlPrintStats();
    return(0.0);
}
