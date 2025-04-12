#include <Zmq/Zmq.mqh>

// Configuration
input string HOST = "127.0.0.1";
input int PORT = 5555;

// ZMQ components
Context context("MQL5 Server");
Socket *pairSocket;

// Subscription data
string symbolToStream = "";
bool subscribed = false;

//+------------------------------------------------------------------+
//| Initialize the ZMQ server                                         |
//+------------------------------------------------------------------+
int OnInit()
{
   // Create and bind PAIR socket (bidirectional communication on single port)
   pairSocket = new Socket(context, ZMQ_PAIR);
   if (!pairSocket.bind(StringFormat("tcp://%s:%d", HOST, PORT))) {
      Print("❌ Failed to bind PAIR socket to port ", PORT);
      return INIT_FAILED;
   }

   Print("✅ MQL5 ZMQ Server initialized on port ", PORT);
   return INIT_SUCCEEDED;
}

//+------------------------------------------------------------------+
//| Process incoming commands and send market data                    |
//+------------------------------------------------------------------+
void OnTick()
{
   // Check for incoming commands
   ProcessIncomingCommands();

   // Stream data immediately on every tick if subscribed
   if (subscribed) {
      SendMarketData();
   }
}

//+------------------------------------------------------------------+
//| Process commands from clients                                     |
//+------------------------------------------------------------------+
void ProcessIncomingCommands()
{
   uchar buffer[];
   ZmqMsg message;

   // Try to receive a message (non-blocking)
   if (pairSocket.recv(message, ZMQ_DONTWAIT)) {
      message.getData(buffer);
      string command = CharArrayToString(buffer);
      Print("📥 Received command: ", command);

      // Process subscription command
      if (StringFind(command, "subscribe:") == 0) {
         symbolToStream = StringSubstr(command, StringLen("subscribe:"));
         subscribed = true;

         // Send acknowledgment
         string response = StringFormat("Subscribed to %s", symbolToStream);
         uchar responseBuf[];
         StringToCharArray(response, responseBuf);
         pairSocket.send(responseBuf);
         Print("📤 Sent response: ", response);
      }
      // Process unsubscribe command
      else if (command == "unsubscribe") {
         subscribed = false;
         symbolToStream = "";

         // Send acknowledgment
         string response = "Unsubscribed";
         uchar responseBuf[];
         StringToCharArray(response, responseBuf);
         pairSocket.send(responseBuf);
         Print("📤 Sent response: ", response);
      }
      // Process historical data request
      else if (StringFind(command, "history:") == 0) {
         string params[];
         StringSplit(command, ':', params);

         if (ArraySize(params) >= 4) {
            string histSymbol = params[1];
            string timeframe = params[2];
            int numCandles = (int)StringToInteger(params[3]);

            // Get and send historical data
            string historyData = GetHistoricalCandles(histSymbol, timeframe, numCandles);
            uchar historyBuf[];
            StringToCharArray(historyData, historyBuf);
            pairSocket.send(historyBuf);
            //Print("📤 Sent historical data for ", histSymbol, " (", timeframe, ")");
         } else {
            string response = "Invalid history command format. Use: history:SYMBOL:TIMEFRAME:COUNT";
            uchar responseBuf[];
            StringToCharArray(response, responseBuf);
            pairSocket.send(responseBuf);
            //Print("❌ Invalid history command format");
         }
      }
      // Unknown command
      else {
         string response = "Unknown command";
         uchar responseBuf[];
         StringToCharArray(response, responseBuf);
         pairSocket.send(responseBuf);
         //Print("❌ Unknown command: ", command);
      }
   }
}

//+------------------------------------------------------------------+
//| Get historical candle data                                        |
//+------------------------------------------------------------------+
string GetHistoricalCandles(string symbol, string tf_str, int count)
{
   // Convert timeframe string to ENUM_TIMEFRAMES
   ENUM_TIMEFRAMES timeframe = StringToTimeFrame(tf_str);
   if (timeframe == PERIOD_CURRENT && tf_str != "CURRENT") {
      return "{\"error\":\"Invalid timeframe: " + tf_str + "\"}";
   }

   // Prepare arrays for candle data
   MqlRates rates[];
   ArraySetAsSeries(rates, true);

   // Fetch historical data
   int copied = CopyRates(symbol, timeframe, 0, count, rates);
   if (copied <= 0) {
      return "{\"error\":\"Failed to copy rate data for " + symbol + "\"}";
   }

   // Build JSON array of candles
   string json = "{\"symbol\":\"" + symbol + "\",\"timeframe\":\"" + tf_str + "\",\"candles\":[";

   for (int i = 0; i < copied; i++) {
      // Add comma if not the first item
      if (i > 0) json += ",";

      // Format candle data as JSON object
      json += StringFormat(
         "{\"time\":%d,\"open\":%.5f,\"high\":%.5f,\"low\":%.5f,\"close\":%.5f,\"tick_volume\":%d,\"spread\":%d,\"real_volume\":%d}",
         rates[i].time,
         rates[i].open,
         rates[i].high,
         rates[i].low,
         rates[i].close,
         rates[i].tick_volume,
         rates[i].spread,
         rates[i].real_volume
      );
   }

   json += "]}";
   return json;
}

//+------------------------------------------------------------------+
//| Convert timeframe string to ENUM_TIMEFRAMES                       |
//+------------------------------------------------------------------+
ENUM_TIMEFRAMES StringToTimeFrame(string tf_str)
{
   if (tf_str == "M1")  return PERIOD_M1;
   if (tf_str == "M5")  return PERIOD_M5;
   if (tf_str == "M15") return PERIOD_M15;
   if (tf_str == "M30") return PERIOD_M30;
   if (tf_str == "H1")  return PERIOD_H1;
   if (tf_str == "H4")  return PERIOD_H4;
   if (tf_str == "D1")  return PERIOD_D1;
   if (tf_str == "W1")  return PERIOD_W1;
   if (tf_str == "MN1") return PERIOD_MN1;
   if (tf_str == "CURRENT") return PERIOD_CURRENT;

   // Default case - return current timeframe
   Print("⚠️ Unknown timeframe string: ", tf_str, ", using PERIOD_CURRENT");
   return PERIOD_CURRENT;
}

//+------------------------------------------------------------------+
//| Send market data to subscribed clients                            |
//+------------------------------------------------------------------+
void SendMarketData()
{

   // Get market data - only bid, ask and timestamp
   double bid = SymbolInfoDouble(symbolToStream, SYMBOL_BID);
   double ask = SymbolInfoDouble(symbolToStream, SYMBOL_ASK);
   datetime server_time = TimeCurrent();

   // Format as JSON with only the needed fields
   string json = StringFormat(
      "{\"type\":\"market_data\",\"symbol\":\"%s\",\"bid\":%.5f,\"ask\":%.5f,\"time\":%d}",
      symbolToStream, bid, ask, server_time
   );

   // Send data
   uchar outBuf[];
   StringToCharArray(json, outBuf);
   pairSocket.send(outBuf);
   Print("📊 Sent market data: ", json);
}


//+------------------------------------------------------------------+
//| Clean up when expert advisor is removed                           |
//+------------------------------------------------------------------+
void OnDeinit(const int reason)
{
   if (pairSocket != NULL) {
      delete pairSocket;
      pairSocket = NULL;
   }

   context.shutdown();
   context.destroy(0);
   Print("👋 ZMQ Server shutdown complete.");
}
