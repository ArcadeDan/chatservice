#include <stdio.h>
#include "winsock2.h"

#define SERVER_PORT  9999
#define MAX_LINE      256

   void main(int argc, char **argv) {
   
      if (argc < 2){
         printf("\nUseage: client serverName\n");
         return;
      }
   
    // Initialize Winsock.
      WSADATA wsaData;
      int iResult = WSAStartup( MAKEWORD(2,2), &wsaData );
      if ( iResult != NO_ERROR ){
         printf("Error at WSAStartup()\n");
         return;
      }
   
   	//translate the server name or IP address (128.90.54.1) to resolved IP address
      unsigned int ipaddr;
   	// If the user input is an alpha name for the host, use gethostbyname()
   	// If not, get host by addr (assume IPv4)
      if (isalpha(argv[1][0])) {   // host address is a name  
         hostent* remoteHost = gethostbyname(argv[1]);
         if ( remoteHost == NULL){
            printf("Host not found\n");
            WSACleanup(); 
            return;
         }
         ipaddr = *((unsigned long *) remoteHost->h_addr);
      }
      else //"128.90.54.1"
         ipaddr = inet_addr(argv[1]);
   
   
    // Create a socket.
      SOCKET s;
      s = socket( AF_INET, SOCK_STREAM, IPPROTO_TCP );
      if ( s == INVALID_SOCKET ) {
         printf( "Error at socket(): %ld\n", WSAGetLastError() );
         WSACleanup();
         return;
      }
   
    // Connect to a server.
      sockaddr_in addr;
      addr.sin_family = AF_INET;
      addr.sin_addr.s_addr = ipaddr;
      addr.sin_port = htons( SERVER_PORT );
      if ( connect( s, (SOCKADDR*) &addr, sizeof(addr) ) == SOCKET_ERROR) {
         printf( "Failed to connect.\n" );
         WSACleanup();
         return;
      }
   
    // Send and receive data.
      char buf[MAX_LINE];       
      printf("Type whatever you want: ");
      fgets(buf, sizeof(buf),stdin);
      send(s, buf, strlen(buf), 0); 
      int len = recv(s, buf, MAX_LINE, 0); 
      buf[len] = 0;
      printf("Server says: %s\n", buf);
   
      closesocket(s);
   }
