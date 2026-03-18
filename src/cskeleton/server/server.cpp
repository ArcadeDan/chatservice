#include <stdio.h>
#include "winsock2.h"

#define SERVER_PORT   9999
#define MAX_PENDING   5
#define MAX_LINE      256


   void main() {
   
    // Initialize Winsock.
      WSADATA wsaData;
      int iResult = WSAStartup( MAKEWORD(2,2), &wsaData );
      if ( iResult != NO_ERROR ){
         printf("Error at WSAStartup()\n");
         return;
      }
   
    // Create a socket.
      SOCKET listenSocket;
      listenSocket = socket( AF_INET, SOCK_STREAM, IPPROTO_TCP );
   
      if ( listenSocket == INVALID_SOCKET ) {
         printf( "Error at socket(): %ld\n", WSAGetLastError() );
         WSACleanup();
         return;
      }
   
    // Bind the socket.
      sockaddr_in addr;
      addr.sin_family = AF_INET;
      addr.sin_addr.s_addr = INADDR_ANY; //use local address
      addr.sin_port = htons(SERVER_PORT);
      if ( bind( listenSocket, (SOCKADDR*) &addr, sizeof(addr) ) == SOCKET_ERROR ) {
         printf( "bind() failed.\n" );
         closesocket(listenSocket);
         WSACleanup();
         return;
      }
   
    // Listen on the Socket.
      if ( listen( listenSocket, MAX_PENDING ) == SOCKET_ERROR ){
         printf( "Error listening on socket.\n");
         closesocket(listenSocket);
         WSACleanup();
         return;
      }
   
    // Accept connections.
      SOCKET s;
   
      printf( "Waiting for a client to connect...\n" );
      while(1){
         s = accept( listenSocket, NULL, NULL );
         if( s == SOCKET_ERROR){
            printf("accept() error \n");
            closesocket(listenSocket);
            WSACleanup();
            return;
         }
      
         printf( "Client Connected.\n");
      
      	// Send and receive data.
         char buf[MAX_LINE];
         int len = recv( s, buf, MAX_LINE, 0 );
         buf[len] = 0;
         send( s, buf, strlen(buf), 0 );
         closesocket(s);
         printf( "Client Closed.\n");
      }
   
      closesocket(listenSocket);
   }


