//+------------------------------------------------------------------+
//| ZedMqlStats.mqh                                                  |
//| Prints the Strategy Tester statistics as one log line that the   |
//| zed-mql "MQL: Backtest" task reads. Add to your EA:              |
//|   #include <ZedMqlStats.mqh>                                     |
//|                                                                  |
//| In OnInit:                                                       |
//|   ZedMqlInitEquity(ZEDMQL_SAMPLE_BAR);                           |
//| In OnTick (at the very beginning):                               |
//|   ZedMqlSampleEquity();                                          |
//| In OnTester:                                                     |
//|   ZedMqlPrintStats();                                            |
//| In OnDeinit:                                                     |
//|   ZedMqlDeinitEquity();                                          |
//|                                                                  |
//| It also writes the deal history and equity to                    |
//| <common>/Files/zedmql_<EA>_deals.csv and _equity.csv for the     |
//| HTML report. Only active inside the tester; has no effect on     |
//| trading.                                                         |
//+------------------------------------------------------------------+
#ifndef ZED_MQL_STATS_MQH
#define ZED_MQL_STATS_MQH

void ZedMqlWriteDeals()
  {
   if(!MQLInfoInteger(MQL_TESTER)) return;
   if(!HistorySelect(0,TimeCurrent()))
      return;
   string name="zedmql_"+MQLInfoString(MQL_PROGRAM_NAME)+"_deals.csv";
   int h=FileOpen(name,FILE_WRITE|FILE_COMMON|FILE_TXT|FILE_ANSI);
   if(h==INVALID_HANDLE)
     {
      PrintFormat("ZEDMQL_DEALS error=%d",GetLastError());
      return;
     }
   FileWriteString(h,"time,ticket,position_id,type,entry,symbol,volume,price,profit,swap,commission,fee\n");
   int rows=0, total=HistoryDealsTotal();
   for(int i=0; i<total; i++)
     {
      ulong t=HistoryDealGetTicket(i);
      if(t==0)
         continue;
      string sym=HistoryDealGetString(t,DEAL_SYMBOL);
      StringReplace(sym,",","");
      FileWriteString(h,StringFormat("%I64d,%I64u,%I64u,%d,%d,%s,%.3f,%.5f,%.2f,%.2f,%.2f,%.2f\n",
                      (long)HistoryDealGetInteger(t,DEAL_TIME),t,
                      (long)HistoryDealGetInteger(t,DEAL_POSITION_ID),
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

enum ENUM_ZEDMQL_SAMPLE_MODE
  {
   ZEDMQL_SAMPLE_BAR,
   ZEDMQL_SAMPLE_TICK,
   ZEDMQL_SAMPLE_TIME
  };

string zedmql_equity_file = "";
int zedmql_equity_handle = INVALID_HANDLE;
ENUM_ZEDMQL_SAMPLE_MODE zedmql_sample_mode = ZEDMQL_SAMPLE_BAR;
int zedmql_sample_interval = 0;
datetime zedmql_last_sample_time = 0;
int zedmql_sample_count = 0;

void ZedMqlInitEquity(ENUM_ZEDMQL_SAMPLE_MODE mode = ZEDMQL_SAMPLE_BAR, int interval_seconds = 3600)
  {
   if(!MQLInfoInteger(MQL_TESTER)) return;
   zedmql_sample_mode = mode;
   zedmql_sample_interval = interval_seconds;
   zedmql_equity_file = "zedmql_" + MQLInfoString(MQL_PROGRAM_NAME) + "_equity.csv";
   zedmql_equity_handle = FileOpen(zedmql_equity_file, FILE_WRITE|FILE_COMMON|FILE_TXT|FILE_ANSI);
   if(zedmql_equity_handle != INVALID_HANDLE)
     {
      FileWriteString(zedmql_equity_handle, "time,balance,equity,margin,free_margin\n");
      zedmql_sample_count = 0;
     }
  }

void ZedMqlSampleEquity()
  {
   if(!MQLInfoInteger(MQL_TESTER) || zedmql_equity_handle == INVALID_HANDLE) return;
   
   datetime t = TimeCurrent();
   bool should_sample = false;
   
   if(zedmql_sample_mode == ZEDMQL_SAMPLE_TICK)
      should_sample = true;
   else if(zedmql_sample_mode == ZEDMQL_SAMPLE_BAR)
     {
      static datetime last_bar = 0;
      datetime current_bar = (datetime)SeriesInfoInteger(_Symbol, _Period, SERIES_LASTBAR_DATE);
      if(current_bar != last_bar)
        {
         should_sample = true;
         last_bar = current_bar;
        }
     }
   else if(zedmql_sample_mode == ZEDMQL_SAMPLE_TIME)
     {
      if(t >= zedmql_last_sample_time + zedmql_sample_interval)
         should_sample = true;
     }
     
   if(should_sample)
     {
      double bal = AccountInfoDouble(ACCOUNT_BALANCE);
      double eq = AccountInfoDouble(ACCOUNT_EQUITY);
      double mar = AccountInfoDouble(ACCOUNT_MARGIN);
      double fmar = AccountInfoDouble(ACCOUNT_MARGIN_FREE);
      
      FileWriteString(zedmql_equity_handle, StringFormat("%I64d,%.2f,%.2f,%.2f,%.2f\n", 
                                                         (long)t, bal, eq, mar, fmar));
      zedmql_last_sample_time = t;
      zedmql_sample_count++;
      
      // Buffer file writes for performance
      if(zedmql_sample_count % 100 == 0)
         FileFlush(zedmql_equity_handle);
     }
  }

void ZedMqlDeinitEquity()
  {
   if(zedmql_equity_handle != INVALID_HANDLE)
     {
      FileClose(zedmql_equity_handle);
      zedmql_equity_handle = INVALID_HANDLE;
     }
  }

#endif
