// Fill out your copyright notice in the Description page of Project Settings.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/PlayerState.h"
#include "HomagePlayerState.generated.h"

/**
 * 
 */
UCLASS()
class UNREALGAME_API AHomagePlayerState : public APlayerState
{
	GENERATED_BODY()
	
	UPROPERTY(Replicated)
	uint8 TeamNumber;
};
