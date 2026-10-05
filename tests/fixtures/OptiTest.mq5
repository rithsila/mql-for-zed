//+------------------------------------------------------------------+
//|                                                     OptiTest.mq5 |
//+------------------------------------------------------------------+
#property copyright "Copyright 2024"
#property link      "https://example.com"
#property version   "1.00"

input int InpMA_Period = 14; // MA Period
input double InpLotSize = 0.1; // Lot Size

int OnInit()
  {
   return(INIT_SUCCEEDED);
  }

void OnDeinit(const int reason)
  {
  }

void OnTick()
  {
  }
