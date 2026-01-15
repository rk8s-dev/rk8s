# https://juejin.cn/post/7083799151162425357
# author: Phoenix_Zenghao

from scipy.io import loadmat
import numpy as np
from scipy.optimize import minimize

def load_data(path):
    data=loadmat(path)
    X=data["X"]
    y=data["y"]
    return X,y

def sigmoid(z):
    return 1/(1+np.exp(-z))

def regularized_cost(Theta,X,y,l):
    ThetaReg=Theta[1:]
    cost=(-y*np.log(sigmoid(X@Theta)))-(1-y)*np.log(1-sigmoid((X@Theta)))
    reg=(ThetaReg@ThetaReg)*l/(2*len(X))
    return np.mean(cost)+reg

def regularized_gradient(Theta,X,y,l):
    ThetaReg=Theta[1:]
    cost=(X.T@(sigmoid(X@Theta)-y))*(1/len(X))
    reg=np.concatenate([np.array([0]),(l/len(X))*ThetaReg])
    return cost+reg

def one_vs_all(X,y,l,K):
    All_Theta=np.zeros((K,X.shape[1]))
    for i in range(1,K+1):
        Theta=np.zeros(X.shape[1])
        y_i=np.array([1 if labal==i else 0 for labal in y])
        ret=minimize(fun=regularized_cost, x0=Theta,args=(X,y_i,l),method='TNC',jac=regularized_gradient)
        All_Theta[i-1:]=ret.x
    return All_Theta

def predict(X,All_Theta):
    h=sigmoid(X@All_Theta.T)
    h_argmax=np.argmax(h,axis=1)
    h_argmax=h_argmax+1
    return h_argmax


# np.set_printoptions(threshold=sys.maxsize)
X, y = load_data('ex3data1.mat')
X=np.insert(X,0,1,axis=1)
y=y.flatten()
All_Theta=one_vs_all(X, y, 1, 10)
y_predict=predict(X, All_Theta)
accuracy=np.mean(y_predict==y)
print("accuracy=%.2f%%"%(accuracy*100))