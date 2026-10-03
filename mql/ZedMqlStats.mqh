//+------------------------------------------------------------------+
//| ZedMqlStats.mqh                                                  |
//| Prints the Strategy Tester statistics as one log line that the   |
//| zed-mql "MQL: Backtest" task reads. Add to your EA:              |
//|   #include <ZedMqlStats.mqh>                                     |
//|   double OnTester() { ZedMqlPrintStats(); ... }                  |
//| It also writes the deal history to                               |
//| <common>/Files/zedmql_<EA>_deals.csv for the HTML report.        |
//| Only active inside the tester; has no effect on trading.        |
//+------------------------------------------------------------------+
#ifndef ZED_MQL_STATS_MQH
#define ZED_MQL_STATS_MQH

void ZedMqlWriteDeals()
  {
   if(!HistorySelect(0,TimeCurrent()))
      return;
   string name="zedmql_"+MQLInfoString(MQL_PROGRAM_NAME)+"_deals.csv";
   int h=FileOpen(name,FILE_WRITE|FILE_COMMON|FILE_TXT|FILE_ANSI);
   if(h==INVALID_HANDLE)
     {
      PrintFormat("ZEDMQL_DEALS error=%d",GetLastError());
      return;
     }
   FileWriteString(h,"time,ticket,type,entry,symbol,volume,price,profit,swap,commission,fee\n");
   int rows=0, total=HistoryDealsTotal();
   for(int i=0; i<total; i++)
     {
      ulong t=HistoryDealGetTicket(i);
      if(t==0)
         continue;
      string sym=HistoryDealGetString(t,DEAL_SYMBOL);
      StringReplace(sym,",","");
      FileWriteString(h,StringFormat("%I64d,%I64u,%d,%d,%s,%.3f,%.5f,%.2f,%.2f,%.2f,%.2f\n",
                      (long)HistoryDealGetInteger(t,DEAL_TIME),t,
                      (int)HistoryDealGetInteger(t,DEAL_TYPE),(int)HistoryDealGetInteger(t,DEAL_ENTRY),sym,
                      HistoryDealGetDouble(t,DEAL_VOLUME),HistoryDealGetDouble(t,DEAL_PRICE),
                      HistoryDealGetDouble(t,DEAL_PROFIT),HistoryDealGetDouble(t,DEAL_SWAP),
                      HistoryDealGetDouble(t,DEAL_COMMISSION),HistoryDealGetDouble(t,DEAL_FEE)));
      rows++;
     }
   FileClose(h);
   PrintFormat("ZEDMQL_DEALS file=%s rows=%d",name,rows);
  }

void ZedMqlPrintStats()
  {
   if(!MQLInfoInteger(MQL_TESTER))
      return;
   PrintFormat("ZEDMQL_STATS profit=%.2f gross_profit=%.2f gross_loss=%.2f profit_factor=%.4f expected_payoff=%.4f "
               "equity_dd=%.2f equity_dd_pct=%.2f recovery_factor=%.4f sharpe=%.4f trades=%d",
               TesterStatistics(STAT_PROFIT),
               TesterStatistics(STAT_GROSS_PROFIT),
               TesterStatistics(STAT_GROSS_LOSS),
               TesterStatistics(STAT_PROFIT_FACTOR),
               TesterStatistics(STAT_EXPECTED_PAYOFF),
               TesterStatistics(STAT_EQUITY_DD),
               TesterStatistics(STAT_EQUITY_DDREL_PERCENT),
               TesterStatistics(STAT_RECOVERY_FACTOR),
               TesterStatistics(STAT_SHARPE_RATIO),
               (int)TesterStatistics(STAT_TRADES));
   ZedMqlWriteDeals();
  }

#endif
